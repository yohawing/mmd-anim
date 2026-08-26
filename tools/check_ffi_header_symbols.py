#!/usr/bin/env python3
"""Verify Rust FFI exports match C header declarations in mmd_runtime.h."""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LIB_RS = ROOT / "crates/mmd-anim-ffi/src/lib.rs"
HEADER = ROOT / "crates/mmd-anim-ffi/include/mmd_runtime.h"

RUST_NO_MANGLE_RE = re.compile(r"#\[\s*(?:unsafe\s*\(\s*)?no_mangle\s*\)?\s*\]")
RUST_FN_RE = re.compile(r"fn\s+(mmd_runtime_\w+)\s*\(")
HEADER_FN_RE = re.compile(r"\b(mmd_runtime_\w+)\s*\(")

FORBIDDEN_REMOVED_SYMBOLS = {
    "mmd_runtime_reduced_pose_sample",
    "mmd_runtime_reduced_pose_unity_curve_count",
    "mmd_runtime_reduced_pose_unity_curve_descriptor",
    "mmd_runtime_reduced_pose_unity_curve_keys",
}

GENERIC_CONSTANTS = {
    "MMD_RUNTIME_REDUCED_POSE_GENERIC_CURVE_ABI_VERSION_V1": 1,
    "MMD_RUNTIME_GENERIC_CURVE_BONE_LOCAL": 0,
    "MMD_RUNTIME_GENERIC_CURVE_MORPH_WEIGHT": 1,
    "MMD_RUNTIME_GENERIC_COORDINATE_MMD_RUNTIME_NATIVE": 0,
    "MMD_RUNTIME_GENERIC_LENGTH_MODEL_UNITS": 0,
    "MMD_RUNTIME_GENERIC_ANGLE_RADIANS": 0,
    "MMD_RUNTIME_GENERIC_TIME_SAMPLE_FRAMES": 0,
    "MMD_RUNTIME_GENERIC_TANGENT_VALUE_PER_SAMPLE_FRAME": 0,
    "MMD_RUNTIME_GENERIC_ROTATION_BASIS_NONE": 0,
    "MMD_RUNTIME_GENERIC_ROTATION_BASIS_RUNTIME_QUATERNION": 1,
    "MMD_RUNTIME_GENERIC_ROTATION_BASIS_EULER_XYZ_RADIANS_PER_FRAME": 2,
}

CLIP_TRACK_CONSTANTS = {
    "MMD_RUNTIME_CLIP_BONE_TRACK_INTROSPECTION_ABI_VERSION_V1": 1,
    "MMD_RUNTIME_CLIP_MORPH_TRACK_INTROSPECTION_ABI_VERSION_V1": 1,
    "MMD_RUNTIME_CLIP_PROPERTY_TRACK_INTROSPECTION_ABI_VERSION_V1": 1,
    "MMD_RUNTIME_VMD_TRACK_KEYFRAME_INTROSPECTION_ABI_VERSION_V1": 1,
    "MMD_RUNTIME_VMD_SHARED_CONTEXT_ABI_VERSION_V1": 1,
    "MMD_RUNTIME_VMD_SHARED_CONTEXT_SUMMARY_ABI_VERSION_V1": 1,
    "MMD_RUNTIME_VMD_SHARED_CONTEXT_BONE_READBACK_ABI_VERSION_V1": 1,
    "MMD_RUNTIME_VMD_SHARED_CONTEXT_RAW_READBACK_ABI_VERSION_V1": 1,
    "MMD_RUNTIME_VMD_SUMMARY_BYTES_ABI_VERSION_V1": 1,
    "MMD_RUNTIME_BONE_TRACK_CURVE_NONE": 0,
    "MMD_RUNTIME_BONE_TRACK_CURVE_CUBIC_BEZIER": 1,
    "MMD_RUNTIME_VMD_CURVE_NONE": 0,
    "MMD_RUNTIME_VMD_CURVE_CUBIC_BEZIER": 1,
}

CLIP_TRACK_FEATURE_BITS = {
    "MMD_RUNTIME_FEATURE_CLIP_BONE_TRACK_INTROSPECTION": 5,
    "MMD_RUNTIME_FEATURE_CLIP_MORPH_TRACK_INTROSPECTION": 6,
    "MMD_RUNTIME_FEATURE_CLIP_PROPERTY_TRACK_INTROSPECTION": 7,
    "MMD_RUNTIME_FEATURE_VMD_TRACK_KEYFRAME_INTROSPECTION": 8,
    "MMD_RUNTIME_FEATURE_VMD_SHARED_CONTEXT": 9,
    "MMD_RUNTIME_FEATURE_VMD_SHARED_CONTEXT_BONE_READBACK": 10,
    "MMD_RUNTIME_FEATURE_VMD_SUMMARY_BYTES": 11,
    "MMD_RUNTIME_FEATURE_VMD_SHARED_CONTEXT_RAW_READBACK": 12,
}

GENERIC_STRUCTS = {
    "MmdRuntimeFfiGenericCurveInfo": (
        "mmd_runtime_ffi_generic_curve_info_t",
        [
            ("struct_size", "u32"),
            ("abi_version", "u32"),
            ("reduction_target", "u32"),
            ("coordinate_system", "u32"),
            ("length_unit", "u32"),
            ("angle_unit", "u32"),
            ("time_unit", "u32"),
            ("tangent_unit", "u32"),
            ("model_identity", "u64"),
            ("start_frame", "f32"),
            ("frame_step", "f32"),
            ("frame_count", "usize"),
            ("bone_count", "usize"),
            ("morph_count", "usize"),
        ],
    ),
    "MmdRuntimeFfiGenericCurveDescriptor": (
        "mmd_runtime_ffi_generic_curve_descriptor_t",
        [
            ("struct_size", "u32"),
            ("abi_version", "u32"),
            ("kind", "u32"),
            ("target_index", "u32"),
            ("parent_index", "i32"),
            ("value_flags", "u32"),
            ("interpolation", "u32"),
            ("rotation_basis", "u32"),
            ("key_count", "usize"),
        ],
    ),
    "MmdRuntimeFfiGenericCurveKey": (
        "mmd_runtime_ffi_generic_curve_key_t",
        [
            ("sample_index", "usize"),
            ("frame", "f32"),
            ("translation_xyz", "f32[3]"),
            ("rotation_xyzw", "f32[4]"),
            ("scalar", "f32"),
            ("segment_prev_out_translation_xyz", "f32[3]"),
            ("segment_current_in_translation_xyz", "f32[3]"),
            ("segment_from_previous_start_euler_xyz", "f32[3]"),
            ("segment_from_previous_end_euler_xyz", "f32[3]"),
            ("segment_prev_out_rotation_xyz", "f32[3]"),
            ("segment_current_in_rotation_xyz", "f32[3]"),
            ("segment_prev_out_scalar", "f32"),
            ("segment_current_in_scalar", "f32"),
        ],
    ),
}

CLIP_TRACK_STRUCTS = {
    "MmdRuntimeFfiBoneTrackCurve": (
        "mmd_runtime_ffi_bone_track_curve_t",
        [
            ("kind", "u32"),
            ("x1", "f32"),
            ("y1", "f32"),
            ("x2", "f32"),
            ("y2", "f32"),
        ],
    ),
    "MmdRuntimeFfiBoneTrackDescriptor": (
        "mmd_runtime_ffi_bone_track_descriptor_t",
        [("bone_index", "u32"), ("key_count", "usize")],
    ),
    "MmdRuntimeFfiBoneTrackKey": (
        "mmd_runtime_ffi_bone_track_key_t",
        [
            ("bone_index", "u32"),
            ("frame", "u32"),
            ("position_xyz", "f32[3]"),
            ("rotation_xyzw", "f32[4]"),
            ("translation_x", "bone_track_curve"),
            ("translation_y", "bone_track_curve"),
            ("translation_z", "bone_track_curve"),
            ("rotation", "bone_track_curve"),
        ],
    ),
    "MmdRuntimeFfiMorphTrackDescriptor": (
        "mmd_runtime_ffi_morph_track_descriptor_t",
        [("morph_index", "u32"), ("key_count", "usize")],
    ),
    "MmdRuntimeFfiMorphTrackKey": (
        "mmd_runtime_ffi_morph_track_key_t",
        [("morph_index", "u32"), ("frame", "u32"), ("weight", "f32")],
    ),
    "MmdRuntimeFfiPropertyTrackDescriptor": (
        "mmd_runtime_ffi_property_track_descriptor_t",
        [("key_count", "usize"), ("ik_enabled_count", "usize")],
    ),
    "MmdRuntimeFfiPropertyTrackKey": (
        "mmd_runtime_ffi_property_track_key_t",
        [
            ("frame", "u32"),
            ("ik_enabled_offset", "usize"),
            ("ik_enabled_count", "usize"),
        ],
    ),
    "MmdRuntimeFfiVmdCurve": (
        "mmd_runtime_ffi_vmd_curve_t",
        [("kind", "u32"), ("x1", "f32"), ("y1", "f32"), ("x2", "f32"), ("y2", "f32")],
    ),
    "MmdRuntimeFfiVmdCameraKeyframe": (
        "mmd_runtime_ffi_vmd_camera_keyframe_t",
        [
            ("frame", "u32"),
            ("distance", "f32"),
            ("position_xyz", "f32[3]"),
            ("rotation_xyz", "f32[3]"),
            ("interpolation", "u8[24]"),
            ("fov", "u32"),
            ("perspective", "u8"),
            ("position_x", "vmd_curve"),
            ("position_y", "vmd_curve"),
            ("position_z", "vmd_curve"),
            ("rotation", "vmd_curve"),
            ("distance_curve", "vmd_curve"),
            ("fov_curve", "vmd_curve"),
        ],
    ),
    "MmdRuntimeFfiVmdBoneKeyframe": (
        "mmd_runtime_ffi_vmd_bone_keyframe_t",
        [
            ("bone_index", "u32"),
            ("frame", "u32"),
            ("position_xyz", "f32[3]"),
            ("rotation_xyzw", "f32[4]"),
            ("interpolation", "u8[64]"),
        ],
    ),
    "MmdRuntimeFfiVmdLightKeyframe": (
        "mmd_runtime_ffi_vmd_light_keyframe_t",
        [("frame", "u32"), ("color", "f32[3]"), ("direction", "f32[3]")],
    ),
    "MmdRuntimeFfiVmdSelfShadowKeyframe": (
        "mmd_runtime_ffi_vmd_self_shadow_keyframe_t",
        [("frame", "u32"), ("mode", "u8"), ("distance", "f32")],
    ),
}

VMD_SHARED_CONTEXT_STRUCTS = {
    "MmdRuntimeFfiVmdPropertyKeyframe": (
        "mmd_runtime_ffi_vmd_property_keyframe_t",
        [
            ("frame", "u32"),
            ("visible", "u8"),
            ("reserved", "u8[3]"),
            ("ik_entry_offset", "usize"),
            ("ik_entry_count", "usize"),
        ],
    ),
    "MmdRuntimeFfiVmdPropertyIkEntry": (
        "mmd_runtime_ffi_vmd_property_ik_entry_t",
        [("name_bytes", "u8[20]"), ("enabled", "u8"), ("reserved", "u8[3]")],
    ),
    "MmdRuntimeFfiVmdRawBoneKeyframe": (
        "mmd_runtime_ffi_vmd_raw_bone_keyframe_t",
        [
            ("bone_name_bytes", "u8[15]"),
            ("frame", "u32"),
            ("position_xyz", "f32[3]"),
            ("rotation_xyzw", "f32[4]"),
            ("interpolation", "u8[64]"),
        ],
    ),
    "MmdRuntimeFfiVmdRawMorphKeyframe": (
        "mmd_runtime_ffi_vmd_raw_morph_keyframe_t",
        [("morph_name_bytes", "u8[15]"), ("frame", "u32"), ("weight", "f32")],
    ),
    "MmdRuntimeFfiVmdTrackSummary": (
        "mmd_runtime_ffi_vmd_track_summary_t",
        [("track_count", "u32"), ("key_count", "u32")],
    ),
    "MmdRuntimeFfiVmdContextSummary": (
        "mmd_runtime_ffi_vmd_context_summary_t",
        [
            ("struct_size", "u32"),
            ("abi_version", "u32"),
            ("target_model_name_bytes", "u8[20]"),
            ("max_frame", "u32"),
            ("bones", "vmd_track_summary"),
            ("morphs", "vmd_track_summary"),
            ("cameras", "vmd_track_summary"),
            ("lights", "vmd_track_summary"),
            ("self_shadows", "vmd_track_summary"),
            ("properties", "vmd_track_summary"),
            ("property_ik_entry_count", "u32"),
        ],
    ),
}

GENERIC_FUNCTIONS = {
    "mmd_runtime_reduced_pose_generic_curve_info": (
        "status",
        [("pose", "const_reduced_pose_ptr"), ("out_info", "generic_info_ptr")],
    ),
    "mmd_runtime_reduced_pose_generic_curve_count": (
        "status",
        [("pose", "const_reduced_pose_ptr"), ("out_curve_count", "usize_ptr")],
    ),
    "mmd_runtime_reduced_pose_generic_curve_descriptor": (
        "status",
        [
            ("pose", "const_reduced_pose_ptr"),
            ("curve_index", "usize"),
            ("out_descriptor", "generic_descriptor_ptr"),
        ],
    ),
    "mmd_runtime_reduced_pose_generic_curve_keys": (
        "status",
        [
            ("pose", "const_reduced_pose_ptr"),
            ("curve_index", "usize"),
            ("out_keys", "generic_key_ptr"),
            ("out_key_capacity", "usize"),
            ("key_stride_bytes", "usize"),
            ("out_required_count", "usize_ptr"),
        ],
    ),
}

CLIP_TRACK_FUNCTIONS = {
    "mmd_runtime_clip_bone_track_count": (
        "usize",
        [("clip", "const_clip_ptr")],
    ),
    "mmd_runtime_clip_bone_track_descriptor": (
        "status",
        [
            ("clip", "const_clip_ptr"),
            ("track_index", "usize"),
            ("out_descriptor", "bone_track_descriptor_ptr"),
        ],
    ),
    "mmd_runtime_clip_bone_track_key_count": (
        "usize",
        [("clip", "const_clip_ptr"), ("track_index", "usize")],
    ),
    "mmd_runtime_clip_copy_bone_track_keys": (
        "status",
        [
            ("clip", "const_clip_ptr"),
            ("track_index", "usize"),
            ("out_keys", "bone_track_key_ptr"),
            ("out_key_capacity", "usize"),
            ("out_written", "usize_ptr"),
        ],
    ),
    "mmd_runtime_clip_morph_track_count": (
        "usize",
        [("clip", "const_clip_ptr")],
    ),
    "mmd_runtime_clip_morph_track_descriptor": (
        "status",
        [
            ("clip", "const_clip_ptr"),
            ("track_index", "usize"),
            ("out_descriptor", "morph_track_descriptor_ptr"),
        ],
    ),
    "mmd_runtime_clip_morph_track_key_count": (
        "usize",
        [("clip", "const_clip_ptr"), ("track_index", "usize")],
    ),
    "mmd_runtime_clip_copy_morph_track_keys": (
        "status",
        [
            ("clip", "const_clip_ptr"),
            ("track_index", "usize"),
            ("out_keys", "morph_track_key_ptr"),
            ("out_key_capacity", "usize"),
            ("out_written", "usize_ptr"),
        ],
    ),
    "mmd_runtime_clip_property_track_count": (
        "usize",
        [("clip", "const_clip_ptr")],
    ),
    "mmd_runtime_clip_property_track_descriptor": (
        "status",
        [("clip", "const_clip_ptr"), ("out_descriptor", "property_track_descriptor_ptr")],
    ),
    "mmd_runtime_clip_property_track_key_count": (
        "usize",
        [("clip", "const_clip_ptr")],
    ),
    "mmd_runtime_clip_property_track_ik_enabled_count": (
        "usize",
        [("clip", "const_clip_ptr")],
    ),
    "mmd_runtime_clip_copy_property_track_keys": (
        "status",
        [
            ("clip", "const_clip_ptr"),
            ("out_keys", "property_track_key_ptr"),
            ("out_key_capacity", "usize"),
            ("out_written", "usize_ptr"),
        ],
    ),
    "mmd_runtime_clip_copy_property_track_ik_enabled": (
        "status",
        [
            ("clip", "const_clip_ptr"),
            ("out_states", "mut_u8_ptr"),
            ("out_state_capacity", "usize"),
            ("out_written", "usize_ptr"),
        ],
    ),
    "mmd_runtime_vmd_camera_track_copy_keyframes": (
        "status",
        [
            ("track", "const_vmd_camera_ptr"),
            ("out_keys", "vmd_camera_keyframe_ptr"),
            ("out_key_capacity", "usize"),
            ("out_written", "usize_ptr"),
        ],
    ),
    "mmd_runtime_vmd_light_track_copy_keyframes": (
        "status",
        [
            ("track", "const_vmd_light_ptr"),
            ("out_keys", "vmd_light_keyframe_ptr"),
            ("out_key_capacity", "usize"),
            ("out_written", "usize_ptr"),
        ],
    ),
    "mmd_runtime_vmd_self_shadow_track_copy_keyframes": (
        "status",
        [
            ("track", "const_vmd_self_shadow_ptr"),
            ("out_keys", "vmd_self_shadow_keyframe_ptr"),
            ("out_key_capacity", "usize"),
            ("out_written", "usize_ptr"),
        ],
    ),
}

VMD_SHARED_CONTEXT_FUNCTIONS = {
    "mmd_runtime_vmd_context_create_from_vmd_bytes": (
        "mut_vmd_context_ptr",
        [("data", "const_u8_ptr"), ("len", "usize")],
    ),
    "mmd_runtime_vmd_context_free": (
        "void",
        [("context", "mut_vmd_context_ptr")],
    ),
    "mmd_runtime_vmd_context_read_summary": (
        "status",
        [
            ("context", "const_vmd_context_ptr"),
            ("out_summary", "vmd_context_summary_ptr"),
            ("out_summary_size", "usize"),
        ],
    ),
    "mmd_runtime_vmd_summary_read_from_vmd_bytes": (
        "status",
        [
            ("data", "const_u8_ptr"),
            ("data_len", "usize"),
            ("out_summary", "vmd_context_summary_ptr"),
            ("out_summary_size", "usize"),
        ],
    ),
    "mmd_runtime_vmd_context_camera_frame_count": (
        "usize",
        [("context", "const_vmd_context_ptr")],
    ),
    "mmd_runtime_vmd_context_copy_camera_keyframes": (
        "status",
        [
            ("context", "const_vmd_context_ptr"),
            ("out_keys", "vmd_camera_keyframe_ptr"),
            ("out_key_capacity", "usize"),
            ("out_written", "usize_ptr"),
        ],
    ),
    "mmd_runtime_vmd_context_light_frame_count": (
        "usize",
        [("context", "const_vmd_context_ptr")],
    ),
    "mmd_runtime_vmd_context_copy_light_keyframes": (
        "status",
        [
            ("context", "const_vmd_context_ptr"),
            ("out_keys", "vmd_light_keyframe_ptr"),
            ("out_key_capacity", "usize"),
            ("out_written", "usize_ptr"),
        ],
    ),
    "mmd_runtime_vmd_context_self_shadow_frame_count": (
        "usize",
        [("context", "const_vmd_context_ptr")],
    ),
    "mmd_runtime_vmd_context_copy_self_shadow_keyframes": (
        "status",
        [
            ("context", "const_vmd_context_ptr"),
            ("out_keys", "vmd_self_shadow_keyframe_ptr"),
            ("out_key_capacity", "usize"),
            ("out_written", "usize_ptr"),
        ],
    ),
    "mmd_runtime_vmd_context_property_frame_count": (
        "usize",
        [("context", "const_vmd_context_ptr")],
    ),
    "mmd_runtime_vmd_context_property_ik_entry_count": (
        "usize",
        [("context", "const_vmd_context_ptr")],
    ),
    "mmd_runtime_vmd_context_copy_property_keyframes": (
        "status",
        [
            ("context", "const_vmd_context_ptr"),
            ("out_keys", "vmd_property_keyframe_ptr"),
            ("out_key_capacity", "usize"),
            ("out_written", "usize_ptr"),
        ],
    ),
    "mmd_runtime_vmd_context_copy_property_ik_entries": (
        "status",
        [
            ("context", "const_vmd_context_ptr"),
            ("out_entries", "vmd_property_ik_entry_ptr"),
            ("out_entry_capacity", "usize"),
            ("out_written", "usize_ptr"),
        ],
    ),
    "mmd_runtime_vmd_context_bone_keyframe_count_for_model": (
        "usize",
        [
            ("model", "const_model_ptr"),
            ("context", "const_vmd_context_ptr"),
        ],
    ),
    "mmd_runtime_vmd_context_copy_bone_keyframes_for_model": (
        "status",
        [
            ("model", "const_model_ptr"),
            ("context", "const_vmd_context_ptr"),
            ("out_keys", "vmd_bone_keyframe_ptr"),
            ("out_key_capacity", "usize"),
            ("out_written", "usize_ptr"),
            ("out_skipped", "usize_ptr"),
        ],
    ),
    "mmd_runtime_vmd_context_bone_keyframe_count": (
        "usize",
        [("context", "const_vmd_context_ptr")],
    ),
    "mmd_runtime_vmd_context_copy_bone_keyframes": (
        "status",
        [
            ("context", "const_vmd_context_ptr"),
            ("out_keys", "vmd_raw_bone_keyframe_ptr"),
            ("out_key_capacity", "usize"),
            ("out_written", "usize_ptr"),
        ],
    ),
    "mmd_runtime_vmd_context_morph_keyframe_count": (
        "usize",
        [("context", "const_vmd_context_ptr")],
    ),
    "mmd_runtime_vmd_context_copy_morph_keyframes": (
        "status",
        [
            ("context", "const_vmd_context_ptr"),
            ("out_keys", "vmd_raw_morph_keyframe_ptr"),
            ("out_key_capacity", "usize"),
            ("out_written", "usize_ptr"),
        ],
    ),
    "mmd_runtime_clip_create_from_vmd_context_for_model": (
        "mut_clip_ptr",
        [
            ("model", "const_model_ptr"),
            ("context", "const_vmd_context_ptr"),
        ],
    ),
}

PHYSICS_PARAM_FUNCTIONS = {
    "mmd_runtime_physics_params_get_json": (
        "ffi_byte_buffer",
        [("world", "const_physics_world_ptr")],
    ),
    "mmd_runtime_physics_params_set_json": (
        "status",
        [
            ("world", "physics_world_ptr"),
            ("data", "const_u8_ptr"),
            ("len", "usize"),
        ],
    ),
}

VMD_FROM_PARTS_FUNCTIONS = {
    "mmd_runtime_export_vmd_from_parts": (
        "ffi_byte_buffer",
        [
            ("metadata_json", "const_u8_ptr"),
            ("metadata_json_len", "usize"),
            ("bone_name_indices", "const_u32_ptr"),
            ("bone_name_index_count", "usize"),
            ("bone_frames", "const_u32_ptr"),
            ("bone_frame_count", "usize"),
            ("bone_translations_xyz", "const_f32_ptr"),
            ("bone_translation_f32_len", "usize"),
            ("bone_rotations_xyzw", "const_f32_ptr"),
            ("bone_rotation_f32_len", "usize"),
            ("bone_interpolations", "const_u8_ptr"),
            ("bone_interpolation_u8_len", "usize"),
            ("morph_name_indices", "const_u32_ptr"),
            ("morph_name_index_count", "usize"),
            ("morph_frames", "const_u32_ptr"),
            ("morph_frame_count", "usize"),
            ("morph_weights", "const_f32_ptr"),
            ("morph_weight_count", "usize"),
        ],
    ),
}

VPD_JSON_FUNCTIONS = {
    "mmd_runtime_export_vpd_pose_json": (
        "ffi_byte_buffer",
        [("json", "const_u8_ptr"), ("json_len", "usize")],
    ),
    "mmd_runtime_parse_vpd_pose_json": (
        "ffi_byte_buffer",
        [("data", "const_u8_ptr"), ("len", "usize")],
    ),
}


def rust_exported_symbols(text: str) -> set[str]:
    lines = text.splitlines()
    symbols: set[str] = set()
    for index, line in enumerate(lines):
        if RUST_NO_MANGLE_RE.fullmatch(line.strip()) is None:
            continue
        preceding = lines[max(0, index - 3) : index]
        if any(part.strip() == "#[cfg(test)]" for part in preceding):
            continue
        signature_parts: list[str] = []
        cursor = index + 1
        while cursor < len(lines):
            line = lines[cursor]
            if "{" in line:
                signature_parts.append(line.split("{", 1)[0].strip())
                break
            signature_parts.append(line.strip())
            cursor += 1
        else:
            raise ValueError(f"missing function body after no_mangle attribute at line {index + 1}")
        signature = " ".join(part for part in signature_parts if part)
        match = RUST_FN_RE.search(signature)
        if match is None:
            raise ValueError(f"could not parse Rust FFI signature near line {index + 1}: {signature!r}")
        symbols.add(match.group(1))
    return symbols


def header_declared_symbols(text: str) -> set[str]:
    stripped = re.sub(r"/\*.*?\*/", "", text, flags=re.DOTALL)
    stripped = re.sub(r"//.*?$", "", stripped, flags=re.MULTILINE)
    compact = " ".join(stripped.split())
    return set(HEADER_FN_RE.findall(compact))


def strip_c_comments(text: str) -> str:
    text = re.sub(r"/\*.*?\*/", "", text, flags=re.DOTALL)
    return re.sub(r"//.*?$", "", text, flags=re.MULTILINE)


def canonical_rust_type(type_name: str) -> str:
    compact = " ".join(type_name.split())
    array_match = re.fullmatch(r"\[(u8|u32|u64|i32|usize|f32);\s*(\d+)\]", compact)
    if array_match:
        return f"{array_match.group(1)}[{array_match.group(2)}]"
    return {
        "u8": "u8",
        "u32": "u32",
        "u64": "u64",
        "i32": "i32",
        "usize": "usize",
        "f32": "f32",
        "bool": "bool",
        "MmdRuntimeStatus": "status",
        "MmdRuntimeFfiByteBuffer": "ffi_byte_buffer",
        "*const MmdRuntimeReducedPose": "const_reduced_pose_ptr",
        "*const MmdRuntimeClip": "const_clip_ptr",
        "*mut MmdRuntimeClip": "mut_clip_ptr",
        "*mut MmdRuntimeVmdContext": "mut_vmd_context_ptr",
        "*const MmdRuntimeModel": "const_model_ptr",
        "*const MmdRuntimeVmdContext": "const_vmd_context_ptr",
        "*const MmdRuntimeVmdCameraTrack": "const_vmd_camera_ptr",
        "*const MmdRuntimeVmdLightTrack": "const_vmd_light_ptr",
        "*const MmdRuntimeVmdSelfShadowTrack": "const_vmd_self_shadow_ptr",
        "*const MmdRuntimePhysicsWorld": "const_physics_world_ptr",
        "*mut MmdRuntimePhysicsWorld": "physics_world_ptr",
        "*const u8": "const_u8_ptr",
        "*const u32": "const_u32_ptr",
        "*const f32": "const_f32_ptr",
        "*mut u8": "mut_u8_ptr",
        "*mut usize": "usize_ptr",
        "*mut MmdRuntimeFfiGenericCurveInfo": "generic_info_ptr",
        "*mut MmdRuntimeFfiGenericCurveDescriptor": "generic_descriptor_ptr",
        "*mut MmdRuntimeFfiGenericCurveKey": "generic_key_ptr",
        "MmdRuntimeFfiBoneTrackCurve": "bone_track_curve",
        "*mut MmdRuntimeFfiBoneTrackDescriptor": "bone_track_descriptor_ptr",
        "*mut MmdRuntimeFfiBoneTrackKey": "bone_track_key_ptr",
        "MmdRuntimeFfiVmdCurve": "vmd_curve",
        "*mut MmdRuntimeFfiMorphTrackDescriptor": "morph_track_descriptor_ptr",
        "*mut MmdRuntimeFfiMorphTrackKey": "morph_track_key_ptr",
        "*mut MmdRuntimeFfiPropertyTrackDescriptor": "property_track_descriptor_ptr",
        "*mut MmdRuntimeFfiPropertyTrackKey": "property_track_key_ptr",
        "*mut MmdRuntimeFfiVmdCameraKeyframe": "vmd_camera_keyframe_ptr",
        "*mut MmdRuntimeFfiVmdBoneKeyframe": "vmd_bone_keyframe_ptr",
        "*mut MmdRuntimeFfiVmdLightKeyframe": "vmd_light_keyframe_ptr",
        "*mut MmdRuntimeFfiVmdSelfShadowKeyframe": "vmd_self_shadow_keyframe_ptr",
        "*mut MmdRuntimeFfiVmdPropertyKeyframe": "vmd_property_keyframe_ptr",
        "*mut MmdRuntimeFfiVmdPropertyIkEntry": "vmd_property_ik_entry_ptr",
        "*mut MmdRuntimeFfiVmdRawBoneKeyframe": "vmd_raw_bone_keyframe_ptr",
        "*mut MmdRuntimeFfiVmdRawMorphKeyframe": "vmd_raw_morph_keyframe_ptr",
        "*mut MmdRuntimeFfiVmdContextSummary": "vmd_context_summary_ptr",
        "MmdRuntimeFfiVmdTrackSummary": "vmd_track_summary",
    }.get(compact, compact)


def canonical_c_type(type_name: str) -> str:
    compact = re.sub(r"\s*\*\s*", "*", " ".join(type_name.split()))
    return {
        "uint8_t": "u8",
        "uint32_t": "u32",
        "uint64_t": "u64",
        "int32_t": "i32",
        "size_t": "usize",
        "float": "f32",
        "bool": "bool",
        "mmd_runtime_status_t": "status",
        "mmd_runtime_ffi_byte_buffer_t": "ffi_byte_buffer",
        "const mmd_runtime_reduced_pose_t*": "const_reduced_pose_ptr",
        "const mmd_runtime_clip_t*": "const_clip_ptr",
        "const mmd_runtime_vmd_camera_track_t*": "const_vmd_camera_ptr",
        "const mmd_runtime_model_t*": "const_model_ptr",
        "const mmd_runtime_vmd_context_t*": "const_vmd_context_ptr",
        "const mmd_runtime_vmd_light_track_t*": "const_vmd_light_ptr",
        "const mmd_runtime_vmd_self_shadow_track_t*": "const_vmd_self_shadow_ptr",
        "const mmd_runtime_physics_world_t*": "const_physics_world_ptr",
        "mmd_runtime_physics_world_t*": "physics_world_ptr",
        "const uint8_t*": "const_u8_ptr",
        "const uint32_t*": "const_u32_ptr",
        "const float*": "const_f32_ptr",
        "uint8_t*": "mut_u8_ptr",
        "size_t*": "usize_ptr",
        "mmd_runtime_ffi_generic_curve_info_t*": "generic_info_ptr",
        "mmd_runtime_ffi_generic_curve_descriptor_t*": "generic_descriptor_ptr",
        "mmd_runtime_ffi_generic_curve_key_t*": "generic_key_ptr",
        "mmd_runtime_ffi_bone_track_descriptor_t*": "bone_track_descriptor_ptr",
        "mmd_runtime_ffi_bone_track_key_t*": "bone_track_key_ptr",
        "mmd_runtime_ffi_bone_track_curve_t": "bone_track_curve",
        "mmd_runtime_ffi_vmd_curve_t": "vmd_curve",
        "mmd_runtime_ffi_morph_track_descriptor_t*": "morph_track_descriptor_ptr",
        "mmd_runtime_ffi_morph_track_key_t*": "morph_track_key_ptr",
        "mmd_runtime_ffi_property_track_descriptor_t*": "property_track_descriptor_ptr",
        "mmd_runtime_ffi_property_track_key_t*": "property_track_key_ptr",
        "mmd_runtime_ffi_vmd_camera_keyframe_t*": "vmd_camera_keyframe_ptr",
        "mmd_runtime_ffi_vmd_bone_keyframe_t*": "vmd_bone_keyframe_ptr",
        "mmd_runtime_ffi_vmd_light_keyframe_t*": "vmd_light_keyframe_ptr",
        "mmd_runtime_ffi_vmd_self_shadow_keyframe_t*": "vmd_self_shadow_keyframe_ptr",
        "mmd_runtime_ffi_vmd_property_keyframe_t*": "vmd_property_keyframe_ptr",
        "mmd_runtime_ffi_vmd_property_ik_entry_t*": "vmd_property_ik_entry_ptr",
        "mmd_runtime_ffi_vmd_raw_bone_keyframe_t*": "vmd_raw_bone_keyframe_ptr",
        "mmd_runtime_ffi_vmd_raw_morph_keyframe_t*": "vmd_raw_morph_keyframe_ptr",
        "mmd_runtime_ffi_vmd_context_summary_t*": "vmd_context_summary_ptr",
        "mmd_runtime_ffi_vmd_track_summary_t": "vmd_track_summary",
        "mmd_runtime_vmd_context_t*": "mut_vmd_context_ptr",
        "mmd_runtime_clip_t*": "mut_clip_ptr",
    }.get(compact, compact)


def rust_struct_fields(text: str, name: str) -> list[tuple[str, str]]:
    match = re.search(rf"pub struct {re.escape(name)}\s*\{{(.*?)\n\}}", text, re.DOTALL)
    if match is None:
        raise ValueError(f"missing Rust struct {name}")
    return [
        (field, canonical_rust_type(type_name))
        for field, type_name in re.findall(r"pub\s+(\w+)\s*:\s*([^,]+),", match.group(1))
    ]


def c_struct_fields(text: str, alias: str) -> list[tuple[str, str]]:
    stripped = strip_c_comments(text)
    tag = alias.removesuffix("_t")
    match = re.search(
        rf"typedef struct\s+{re.escape(tag)}\s*\{{(.*?)\}}\s*{re.escape(alias)}\s*;",
        stripped,
        re.DOTALL,
    )
    if match is None:
        raise ValueError(f"missing C struct {alias}")
    fields: list[tuple[str, str]] = []
    for declaration in match.group(1).split(";"):
        declaration = declaration.strip()
        if not declaration:
            continue
        field_match = re.fullmatch(r"(.+?)\s+(\w+)(?:\[(\d+)\])?", declaration)
        if field_match is None:
            raise ValueError(f"could not parse C field in {alias}: {declaration!r}")
        field_type = canonical_c_type(field_match.group(1))
        if field_match.group(3):
            field_type = f"{field_type}[{field_match.group(3)}]"
        fields.append((field_match.group(2), field_type))
    return fields


def rust_function_shape(text: str, name: str) -> tuple[str, list[tuple[str, str]]]:
    match = re.search(
        rf'pub unsafe extern "C" fn {re.escape(name)}\s*\((.*?)\)\s*(?:->\s*([^{{]+?))?\s*\{{',
        text,
        re.DOTALL,
    )
    if match is None:
        raise ValueError(f"missing Rust function {name}")
    params = [
        (param_name, canonical_rust_type(type_name))
        for param_name, type_name in re.findall(r"(\w+)\s*:\s*([^,]+),?", match.group(1))
    ]
    return canonical_rust_type(match.group(2).strip()) if match.group(2) else "void", params


def c_function_shape(text: str, name: str) -> tuple[str, list[tuple[str, str]]]:
    stripped = strip_c_comments(text)
    match = re.search(
        rf"((?:\w+\s*\*)|\w+)\s+{re.escape(name)}\s*\((.*?)\)\s*;",
        stripped,
        re.DOTALL,
    )
    if match is None:
        raise ValueError(f"missing C function {name}")
    params: list[tuple[str, str]] = []
    for declaration in match.group(2).split(","):
        declaration = declaration.strip()
        param_match = re.fullmatch(r"(.+?[\s*])(\w+)", declaration)
        if param_match is None:
            raise ValueError(f"could not parse C parameter in {name}: {declaration!r}")
        params.append((param_match.group(2), canonical_c_type(param_match.group(1))))
    return canonical_c_type(match.group(1)), params


def check_abi_shapes(rust_text: str, header_text: str) -> list[str]:
    errors: list[str] = []
    for name, expected in GENERIC_CONSTANTS.items():
        rust_match = re.search(rf"pub const {name}: u32 = (\d+);", rust_text)
        header_match = re.search(rf"\b{name}(?:\s+|\s*=\s*)(\d+)(?:u)?\b", header_text)
        rust_value = int(rust_match.group(1)) if rust_match else None
        header_value = int(header_match.group(1)) if header_match else None
        if rust_value != expected or header_value != expected or rust_value != header_value:
            errors.append(
                f"constant {name}: Rust={rust_value}, header={header_value}, expected={expected}"
            )
    for name, expected in CLIP_TRACK_CONSTANTS.items():
        rust_match = re.search(rf"pub const {name}: u32 = (\d+);", rust_text)
        header_match = re.search(rf"\b{name}(?:\s+|\s*=\s*)(\d+)(?:u)?\b", header_text)
        rust_value = int(rust_match.group(1)) if rust_match else None
        header_value = int(header_match.group(1)) if header_match else None
        if rust_value != expected or header_value != expected or rust_value != header_value:
            errors.append(
                f"constant {name}: Rust={rust_value}, header={header_value}, expected={expected}"
            )
    for name, expected_bit in CLIP_TRACK_FEATURE_BITS.items():
        rust_match = re.search(
            rf"pub const {name}: u32 = 1\s*<<\s*(\d+);", rust_text
        )
        header_match = re.search(
            rf"#define\s+{name}\s+\(1u\s*<<\s*(\d+)\)", header_text
        )
        rust_bit = int(rust_match.group(1)) if rust_match else None
        header_bit = int(header_match.group(1)) if header_match else None
        if rust_bit != expected_bit or header_bit != expected_bit or rust_bit != header_bit:
            errors.append(
                f"feature {name}: Rust bit={rust_bit}, header bit={header_bit}, expected={expected_bit}"
            )
    for rust_name, (c_alias, expected) in GENERIC_STRUCTS.items():
        rust_fields = rust_struct_fields(rust_text, rust_name)
        c_fields = c_struct_fields(header_text, c_alias)
        if rust_fields != expected or c_fields != expected or rust_fields != c_fields:
            errors.append(
                f"struct {rust_name}/{c_alias}: Rust={rust_fields}, header={c_fields}, expected={expected}"
            )
    for rust_name, (c_alias, expected) in CLIP_TRACK_STRUCTS.items():
        rust_fields = rust_struct_fields(rust_text, rust_name)
        c_fields = c_struct_fields(header_text, c_alias)
        if rust_fields != expected or c_fields != expected or rust_fields != c_fields:
            errors.append(
                f"struct {rust_name}/{c_alias}: Rust={rust_fields}, header={c_fields}, expected={expected}"
            )
    for rust_name, (c_alias, expected) in VMD_SHARED_CONTEXT_STRUCTS.items():
        rust_fields = rust_struct_fields(rust_text, rust_name)
        c_fields = c_struct_fields(header_text, c_alias)
        if rust_fields != expected or c_fields != expected or rust_fields != c_fields:
            errors.append(
                f"struct {rust_name}/{c_alias}: Rust={rust_fields}, header={c_fields}, expected={expected}"
            )
    function_groups = (
        GENERIC_FUNCTIONS,
        CLIP_TRACK_FUNCTIONS,
        VMD_SHARED_CONTEXT_FUNCTIONS,
        PHYSICS_PARAM_FUNCTIONS,
        VMD_FROM_PARTS_FUNCTIONS,
        VPD_JSON_FUNCTIONS,
    )
    for functions in function_groups:
        for name, expected in functions.items():
            rust_shape = rust_function_shape(rust_text, name)
            c_shape = c_function_shape(header_text, name)
            if rust_shape != expected or c_shape != expected or rust_shape != c_shape:
                errors.append(
                    f"function {name}: Rust={rust_shape}, header={c_shape}, expected={expected}"
                )
    return errors


def main() -> int:
    rust_text = LIB_RS.read_text(encoding="utf-8")
    header_text = HEADER.read_text(encoding="utf-8")

    rust_symbols = rust_exported_symbols(rust_text)
    header_symbols = header_declared_symbols(header_text)

    missing_in_header = sorted(rust_symbols - header_symbols)
    missing_in_rust = sorted(header_symbols - rust_symbols)
    forbidden_in_rust = sorted(FORBIDDEN_REMOVED_SYMBOLS & rust_symbols)
    forbidden_in_header = sorted(FORBIDDEN_REMOVED_SYMBOLS & header_symbols)
    shape_errors = check_abi_shapes(rust_text, header_text)

    if missing_in_header or missing_in_rust or forbidden_in_rust or forbidden_in_header or shape_errors:
        print("FFI header symbol mismatch detected.", file=sys.stderr)
        if missing_in_header:
            print("\nExported in Rust but missing from mmd_runtime.h:", file=sys.stderr)
            for symbol in missing_in_header:
                print(f"  - {symbol}", file=sys.stderr)
        if missing_in_rust:
            print("\nDeclared in mmd_runtime.h but missing from Rust exports:", file=sys.stderr)
            for symbol in missing_in_rust:
                print(f"  - {symbol}", file=sys.stderr)
        if forbidden_in_rust or forbidden_in_header:
            print("\nDense reduced-pose output must not be public:", file=sys.stderr)
            for symbol in sorted(set(forbidden_in_rust + forbidden_in_header)):
                print(f"  - {symbol}", file=sys.stderr)
        if shape_errors:
            print("\nTracked reduced-curve ABI shape drift:", file=sys.stderr)
            for error in shape_errors:
                print(f"  - {error}", file=sys.stderr)
        return 1

    print(
        f"OK: {len(rust_symbols)} Rust FFI exports and tracked ABI shapes "
        "match mmd_runtime.h declarations."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
