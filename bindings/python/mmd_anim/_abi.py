"""Machine-readable ctypes subset of the experimental C ABI v2.

This module owns the existing handwritten ABI subset and loads the shared
model-descriptor v1 manifest used to bind and check that declaration.
"""

from __future__ import annotations

import ctypes
import json
from pathlib import Path
from typing import TypeAlias


FieldSpec: TypeAlias = tuple[str, str]
FunctionSpec: TypeAlias = tuple[str, tuple[str, ...]]


MODEL_DESCRIPTOR_MANIFEST_PATH = (
    Path(__file__).resolve().parents[3]
    / "crates"
    / "mmd-anim-ffi"
    / "abi"
    / "model_descriptor_v1.json"
)
MODEL_DESCRIPTOR_MANIFEST: dict[str, object] = json.loads(
    MODEL_DESCRIPTOR_MANIFEST_PATH.read_text(encoding="utf-8")
)


STRUCT_SPECS: dict[str, tuple[FieldSpec, ...]] = {
    "mmd_runtime_ffi_byte_buffer_t": (
        ("data", "uint8_t*"),
        ("len", "size_t"),
    ),
    "mmd_runtime_ffi_rig_ik_link_t": (
        ("bone_slot", "uint32_t"),
        ("has_angle_limit", "bool"),
        ("angle_limit_min_xyz", "float[3]"),
        ("angle_limit_max_xyz", "float[3]"),
    ),
    "mmd_runtime_ffi_rig_bone_t": (
        ("parent_slot", "int32_t"),
        ("rest_position_xyz", "float[3]"),
        ("flags", "uint32_t"),
        ("fixed_axis_xyz", "float[3]"),
    ),
    "mmd_runtime_ffi_ik_solve_stats_t": (
        ("executed_iterations", "uint32_t"),
        ("link_steps", "uint32_t"),
        ("final_distance", "float"),
        ("break_reason", "uint32_t"),
    ),
}


FUNCTION_SPECS: dict[str, FunctionSpec] = {
    "mmd_runtime_abi_version": ("uint32_t", ()),
    "mmd_runtime_feature_flags": ("uint32_t", ()),
    "mmd_runtime_last_error_message": ("const char*", ()),
    "mmd_runtime_byte_buffer_free": ("void", ("mmd_runtime_ffi_byte_buffer_t",)),
    "mmd_runtime_parse_vmd_json": (
        "mmd_runtime_ffi_byte_buffer_t",
        ("const uint8_t*", "size_t"),
    ),
    "mmd_runtime_parse_pmx_non_geometry_json": (
        "mmd_runtime_ffi_byte_buffer_t",
        ("const uint8_t*", "size_t"),
    ),
    "mmd_runtime_pmx_geometry_create": (
        "mmd_runtime_pmx_geometry_t*",
        ("const uint8_t*", "size_t"),
    ),
    "mmd_runtime_pmx_geometry_free": ("void", ("mmd_runtime_pmx_geometry_t*",)),
    "mmd_runtime_pmx_geometry_positions_buffer": (
        "mmd_runtime_ffi_byte_buffer_t",
        ("const mmd_runtime_pmx_geometry_t*",),
    ),
    "mmd_runtime_model_create": (
        "mmd_runtime_model_t*",
        ("const int32_t*", "const float*", "size_t"),
    ),
    "mmd_runtime_model_create_from_pmx_bytes": (
        "mmd_runtime_model_t*",
        ("const uint8_t*", "size_t"),
    ),
    "mmd_runtime_model_bone_count": (
        "size_t",
        ("const mmd_runtime_model_t*",),
    ),
    "mmd_runtime_model_morph_count": (
        "size_t",
        ("const mmd_runtime_model_t*",),
    ),
    "mmd_runtime_model_free": ("void", ("mmd_runtime_model_t*",)),
    "mmd_runtime_instance_create": (
        "mmd_runtime_instance_t*",
        ("const mmd_runtime_model_t*", "size_t"),
    ),
    "mmd_runtime_instance_create_for_model": (
        "mmd_runtime_instance_t*",
        ("const mmd_runtime_model_t*",),
    ),
    "mmd_runtime_instance_free": ("void", ("mmd_runtime_instance_t*",)),
    "mmd_runtime_instance_evaluate_rest_pose": (
        "bool",
        ("mmd_runtime_instance_t*",),
    ),
    "mmd_runtime_instance_evaluate_clip_frame": (
        "bool",
        ("mmd_runtime_instance_t*", "const mmd_runtime_clip_t*", "float"),
    ),
    "mmd_runtime_instance_world_matrix_f32_len": (
        "size_t",
        ("const mmd_runtime_instance_t*",),
    ),
    "mmd_runtime_instance_copy_world_matrices": (
        "bool",
        ("const mmd_runtime_instance_t*", "float*", "size_t"),
    ),
    "mmd_runtime_instance_skinning_matrix_f32_len": (
        "size_t",
        ("const mmd_runtime_instance_t*",),
    ),
    "mmd_runtime_instance_copy_skinning_matrices": (
        "bool",
        ("const mmd_runtime_instance_t*", "float*", "size_t"),
    ),
    "mmd_runtime_instance_morph_weight_len": (
        "size_t",
        ("const mmd_runtime_instance_t*",),
    ),
    "mmd_runtime_instance_copy_morph_weights": (
        "bool",
        ("const mmd_runtime_instance_t*", "float*", "size_t"),
    ),
    "mmd_runtime_clip_create": (
        "mmd_runtime_clip_t*",
        (
            "const mmd_runtime_ffi_bone_track_t*",
            "size_t",
            "const mmd_runtime_ffi_bone_keyframe_t*",
            "size_t",
            "const mmd_runtime_ffi_morph_track_t*",
            "size_t",
            "const mmd_runtime_ffi_morph_keyframe_t*",
            "size_t",
            "const mmd_runtime_ffi_property_keyframe_t*",
            "size_t",
            "const uint8_t*",
            "size_t",
        ),
    ),
    "mmd_runtime_clip_free": ("void", ("mmd_runtime_clip_t*",)),
    "mmd_runtime_clip_create_from_vmd_bytes_for_model": (
        "mmd_runtime_clip_t*",
        ("const mmd_runtime_model_t*", "const uint8_t*", "size_t"),
    ),
    "mmd_runtime_clip_frame_range": (
        "bool",
        ("const mmd_runtime_clip_t*", "uint32_t*", "uint32_t*"),
    ),
    "mmd_runtime_physics_world_create_from_pmx_bytes": (
        "mmd_runtime_status_t",
        ("const uint8_t*", "size_t", "mmd_runtime_physics_world_t**"),
    ),
    "mmd_runtime_physics_world_free": (
        "void",
        ("mmd_runtime_physics_world_t*",),
    ),
    "mmd_runtime_physics_params_get_json": (
        "mmd_runtime_ffi_byte_buffer_t",
        ("const mmd_runtime_physics_world_t*",),
    ),
    "mmd_runtime_physics_params_set_json": (
        "mmd_runtime_status_t",
        ("mmd_runtime_physics_world_t*", "const uint8_t*", "size_t"),
    ),
    "mmd_runtime_ik_chain_create": (
        "mmd_runtime_ik_chain_t*",
        (
            "const mmd_runtime_ffi_rig_bone_t*",
            "size_t",
            "uint32_t",
            "const mmd_runtime_ffi_rig_ik_link_t*",
            "size_t",
            "uint32_t",
            "float",
        ),
    ),
    "mmd_runtime_ik_chain_free": ("void", ("mmd_runtime_ik_chain_t*",)),
    "mmd_runtime_ik_chain_solve": (
        "bool",
        (
            "mmd_runtime_ik_chain_t*",
            "const float*",
            "const float*",
            "const float*",
            "const float*",
            "float",
            "uint32_t",
            "float*",
            "size_t",
            "mmd_runtime_ffi_ik_solve_stats_t*",
        ),
    ),
}


class ByteBuffer(ctypes.Structure):
    pass


class RigIkLink(ctypes.Structure):
    pass


class RigBone(ctypes.Structure):
    pass


class IkSolveStats(ctypes.Structure):
    pass


class ModelBoneDescriptor(ctypes.Structure):
    pass


class ModelIkSolverDescriptor(ctypes.Structure):
    pass


class ModelIkLinkDescriptor(ctypes.Structure):
    pass


class ModelAppendDescriptor(ctypes.Structure):
    pass


class ModelBoneMorphOffsetDescriptor(ctypes.Structure):
    pass


class ModelGroupMorphOffsetDescriptor(ctypes.Structure):
    pass


class ModelDescriptor(ctypes.Structure):
    pass


STRUCT_TYPES: dict[str, type[ctypes.Structure]] = {
    "mmd_runtime_ffi_byte_buffer_t": ByteBuffer,
    "mmd_runtime_ffi_rig_ik_link_t": RigIkLink,
    "mmd_runtime_ffi_rig_bone_t": RigBone,
    "mmd_runtime_ffi_ik_solve_stats_t": IkSolveStats,
    "mmd_runtime_model_bone_descriptor_t": ModelBoneDescriptor,
    "mmd_runtime_model_ik_solver_descriptor_t": ModelIkSolverDescriptor,
    "mmd_runtime_model_ik_link_descriptor_t": ModelIkLinkDescriptor,
    "mmd_runtime_model_append_descriptor_t": ModelAppendDescriptor,
    "mmd_runtime_model_bone_morph_offset_descriptor_t": ModelBoneMorphOffsetDescriptor,
    "mmd_runtime_model_group_morph_offset_descriptor_t": ModelGroupMorphOffsetDescriptor,
    "mmd_runtime_model_descriptor_t": ModelDescriptor,
}


_SCALARS: dict[str, object] = {
    "void": None,
    "bool": ctypes.c_bool,
    "float": ctypes.c_float,
    "int32_t": ctypes.c_int32,
    "mmd_runtime_status_t": ctypes.c_int,
    "uint8_t": ctypes.c_uint8,
    "uint32_t": ctypes.c_uint32,
    "size_t": ctypes.c_size_t,
}


for _record in MODEL_DESCRIPTOR_MANIFEST["records"]:  # type: ignore[index]
    _name = _record["name"]  # type: ignore[index]
    STRUCT_SPECS[_name] = tuple(  # type: ignore[index]
        (_field["name"], _field["type"])  # type: ignore[index]
        for _field in _record["fields"]  # type: ignore[index]
    )
for _function in MODEL_DESCRIPTOR_MANIFEST["functions"]:  # type: ignore[index]
    FUNCTION_SPECS[_function["name"]] = (  # type: ignore[index]
        _function["return_type"],  # type: ignore[index]
        tuple(  # type: ignore[index]
            _argument["type"]  # type: ignore[index]
            for _argument in _function["arguments"]  # type: ignore[index]
        ),
    )


def ctypes_type(c_type: str) -> object:
    """Resolve one normalized C spelling to its ctypes representation."""

    if c_type == "const char*":
        return ctypes.c_char_p
    if c_type.endswith("]"):
        base, count = c_type[:-1].split("[")
        return ctypes_type(base) * int(count)  # type: ignore[operator]
    if c_type in STRUCT_TYPES:
        return STRUCT_TYPES[c_type]
    if c_type.startswith("const "):
        c_type = c_type.removeprefix("const ")
    if c_type.endswith("**"):
        return ctypes.POINTER(ctypes.c_void_p)
    if c_type.endswith("*"):
        pointee = c_type[:-1]
        if pointee in _SCALARS and _SCALARS[pointee] is not None:
            return ctypes.POINTER(_SCALARS[pointee])
        if pointee in STRUCT_TYPES:
            return ctypes.POINTER(STRUCT_TYPES[pointee])
        return ctypes.c_void_p
    return _SCALARS[c_type]


for _name, _fields in STRUCT_SPECS.items():
    STRUCT_TYPES[_name]._fields_ = [
        (field_name, ctypes_type(field_type)) for field_name, field_type in _fields
    ]


def bind_functions(library: ctypes.CDLL) -> None:
    """Apply all manifest prototypes to a loaded runtime library."""

    for name, (return_type, argument_types) in FUNCTION_SPECS.items():
        try:
            function = getattr(library, name)
        except AttributeError:
            # Model descriptors were added behind the existing ABI v2 feature
            # bit.  Keep older ABI-v2 runtimes usable for the established
            # binding surface; callers must check the feature bit before using
            # this optional constructor.
            if name == "mmd_runtime_model_create_from_descriptor":
                continue
            raise
        function.argtypes = [ctypes_type(value) for value in argument_types]
        function.restype = ctypes_type(return_type)
