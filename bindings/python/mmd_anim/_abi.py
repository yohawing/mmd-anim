"""Manifest-driven ctypes subset of the experimental C ABI v3."""

from __future__ import annotations

import ctypes
import json
from pathlib import Path
from typing import TypeAlias

FieldSpec: TypeAlias = tuple[str, str]
FunctionSpec: TypeAlias = tuple[str, tuple[str, ...]]

_ABI_DIR = Path(__file__).resolve().parents[3] / "crates" / "mmd-anim-ffi" / "abi"
PYTHON_ABI_MANIFEST_PATH = _ABI_DIR / "python_abi_v3.json"
MODEL_DESCRIPTOR_MANIFEST_PATH = _ABI_DIR / "model_descriptor_v1.json"
MODEL_DESCRIPTOR_MANIFEST: dict[str, object] = json.loads(
    MODEL_DESCRIPTOR_MANIFEST_PATH.read_text(encoding="utf-8")
)
_PYTHON_ABI_MANIFEST: dict[str, object] = json.loads(
    PYTHON_ABI_MANIFEST_PATH.read_text(encoding="utf-8")
)

STRUCT_SPECS: dict[str, tuple[FieldSpec, ...]] = {
    name: tuple((field["name"], field["type"]) for field in fields)  # type: ignore[index]
    for name, fields in _PYTHON_ABI_MANIFEST["structs"].items()  # type: ignore[union-attr]
}
FUNCTION_SPECS: dict[str, FunctionSpec] = {
    name: (
        spec["return_type"],  # type: ignore[index]
        tuple(argument["type"] for argument in spec["arguments"]),  # type: ignore[index]
    )
    for name, spec in _PYTHON_ABI_MANIFEST["functions"].items()  # type: ignore[union-attr]
}

# Model-descriptor declarations live in their dedicated manifest because that
# file is also consumed by the C layout checker.  Merge them into the complete
# Python binding surface without duplicating the declarations here.
for _record in MODEL_DESCRIPTOR_MANIFEST["records"]:  # type: ignore[index]
    STRUCT_SPECS[_record["name"]] = tuple(  # type: ignore[index]
        (_field["name"], _field["type"]) for _field in _record["fields"]  # type: ignore[index]
    )
for _function in MODEL_DESCRIPTOR_MANIFEST["functions"]:  # type: ignore[index]
    FUNCTION_SPECS[_function["name"]] = (  # type: ignore[index]
        _function["return_type"],  # type: ignore[index]
        tuple(_argument["type"] for _argument in _function["arguments"]),  # type: ignore[index]
    )


class ByteBuffer(ctypes.Structure):
    pass


class RigIkLink(ctypes.Structure):
    pass


class RigBone(ctypes.Structure):
    pass


class IkSolveStats(ctypes.Structure):
    pass


class PhysicsTickConfig(ctypes.Structure):
    pass


class PhysicsStepStats(ctypes.Structure):
    pass


class PhysicsRigidbodyDescriptor(ctypes.Structure):
    pass


class PhysicsJointDescriptor(ctypes.Structure):
    pass


class PhysicsWorldStepReport(ctypes.Structure):
    pass


class PhysicsRigidbodyBinding(ctypes.Structure):
    pass


class HostPoseView(ctypes.Structure):
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
    "mmd_runtime_ffi_physics_tick_config_t": PhysicsTickConfig,
    "mmd_runtime_ffi_physics_step_stats_t": PhysicsStepStats,
    "mmd_runtime_ffi_physics_rigidbody_desc_t": PhysicsRigidbodyDescriptor,
    "mmd_runtime_ffi_physics_joint_desc_t": PhysicsJointDescriptor,
    "mmd_runtime_ffi_physics_world_step_report_t": PhysicsWorldStepReport,
    "mmd_runtime_ffi_physics_rigidbody_binding_t": PhysicsRigidbodyBinding,
    "mmd_runtime_ffi_host_pose_view_t": HostPoseView,
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
    "uint16_t": ctypes.c_uint16,
    "uint32_t": ctypes.c_uint32,
    "size_t": ctypes.c_size_t,
    "mmd_runtime_physics_frame_action_t": ctypes.c_int,
}


def ctypes_type(c_type: str) -> object:
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
            if name == "mmd_runtime_model_create_from_descriptor":
                continue
            raise
        function.argtypes = [ctypes_type(value) for value in argument_types]
        function.restype = ctypes_type(return_type)
