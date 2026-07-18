"""Typed Python records and the borrowed ctypes carrier for model creation.

The records in this module are intentionally small, immutable values.  A
``_ModelDescriptorCarrier`` owns every pointed-to ctypes array for the whole
native constructor call; the native implementation copies those records before
returning the model handle.
"""

from __future__ import annotations

import ctypes
from dataclasses import dataclass
from typing import Iterable, TypeAlias

from ._abi import (
    MODEL_DESCRIPTOR_MANIFEST,
    ModelAppendDescriptor,
    ModelBoneDescriptor,
    ModelBoneMorphOffsetDescriptor,
    ModelDescriptor,
    ModelGroupMorphOffsetDescriptor,
    ModelIkLinkDescriptor,
    ModelIkSolverDescriptor,
)


_Vec3: TypeAlias = tuple[float, float, float]
_Vec4: TypeAlias = tuple[float, float, float, float]


def _manifest_flag(name: str) -> int:
    flags = MODEL_DESCRIPTOR_MANIFEST["flags"]
    assert isinstance(flags, dict)
    return int(flags[name])


DESCRIPTOR_VERSION = int(MODEL_DESCRIPTOR_MANIFEST["descriptor_version"])
FEATURE_MODEL_DESCRIPTOR = int(
    MODEL_DESCRIPTOR_MANIFEST["feature"]["value"]  # type: ignore[index]
)
MODEL_DESCRIPTOR_FLAGS_NONE = _manifest_flag("MMD_RUNTIME_MODEL_DESCRIPTOR_FLAGS_NONE")
MODEL_BONE_TRANSFORM_AFTER_PHYSICS = _manifest_flag(
    "MMD_RUNTIME_MODEL_BONE_TRANSFORM_AFTER_PHYSICS"
)
MODEL_BONE_FIXED_AXIS = _manifest_flag("MMD_RUNTIME_MODEL_BONE_FIXED_AXIS")
MODEL_BONE_LOCAL_AXIS = _manifest_flag("MMD_RUNTIME_MODEL_BONE_LOCAL_AXIS")
IK_LINK_ANGLE_LIMIT = _manifest_flag("MMD_RUNTIME_MODEL_IK_LINK_ANGLE_LIMIT")
APPEND_ROTATION = _manifest_flag("MMD_RUNTIME_APPEND_ROTATION")
APPEND_TRANSLATION = _manifest_flag("MMD_RUNTIME_APPEND_TRANSLATION")
APPEND_LOCAL = _manifest_flag("MMD_RUNTIME_APPEND_LOCAL")


def _as_tuple(value: Iterable[float], *, name: str, length: int) -> tuple[float, ...]:
    """Copy a tuple-like value without imposing semantic numeric validation."""

    try:
        values = tuple(value)
    except TypeError as error:
        raise TypeError(f"{name} must be an iterable of {length} numbers") from error
    if len(values) != length:
        raise ValueError(f"{name} must contain exactly {length} values")
    return values


def _as_bool(value: object, *, name: str) -> bool:
    # ctypes accepts integers for c_bool, which can silently turn malformed
    # descriptor metadata into a different flag.  Keep the native semantic
    # checks authoritative, but reject this marshal-safety hazard early.
    if type(value) is not bool:
        raise TypeError(f"{name} must be bool")
    return value


def _as_nonnegative_int(value: object, *, name: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int):
        raise TypeError(f"{name} must be an integer")
    if value < 0:
        raise ValueError(f"{name} must be non-negative")
    if value > 0xFFFFFFFF:
        raise ValueError(f"{name} exceeds uint32 range")
    return value


@dataclass(frozen=True)
class Bone:
    """One bone in contiguous native index order.

    ``rest_position_pmx_xyz`` is an absolute PMX-space position.  ``parent``
    accepts either ``None`` or ``-1`` for a root; all other index semantics are
    deliberately checked by the native descriptor compiler.
    """

    parent: int | None
    rest_position_pmx_xyz: _Vec3
    transform_order: int = 0
    transform_after_physics: bool = False
    fixed_axis_xyz: _Vec3 | None = None
    local_axis_x_xyz: _Vec3 | None = None
    local_axis_z_xyz: _Vec3 | None = None

    def __post_init__(self) -> None:
        object.__setattr__(
            self,
            "rest_position_pmx_xyz",
            tuple(self.rest_position_pmx_xyz),
        )
        if self.fixed_axis_xyz is not None:
            object.__setattr__(self, "fixed_axis_xyz", tuple(self.fixed_axis_xyz))
        if self.local_axis_x_xyz is not None:
            object.__setattr__(self, "local_axis_x_xyz", tuple(self.local_axis_x_xyz))
        if self.local_axis_z_xyz is not None:
            object.__setattr__(self, "local_axis_z_xyz", tuple(self.local_axis_z_xyz))

    @property
    def fixed_axis(self) -> _Vec3 | None:
        return self.fixed_axis_xyz

    @property
    def local_axis(self) -> tuple[_Vec3, _Vec3] | None:
        if self.local_axis_x_xyz is None and self.local_axis_z_xyz is None:
            return None
        return self.local_axis_x_xyz, self.local_axis_z_xyz  # type: ignore[return-value]


@dataclass(frozen=True)
class IkLink:
    bone: int
    angle_limit_min_xyz: _Vec3 | None = None
    angle_limit_max_xyz: _Vec3 | None = None

    def __post_init__(self) -> None:
        if self.angle_limit_min_xyz is not None:
            object.__setattr__(
                self, "angle_limit_min_xyz", tuple(self.angle_limit_min_xyz)
            )
        if self.angle_limit_max_xyz is not None:
            object.__setattr__(
                self, "angle_limit_max_xyz", tuple(self.angle_limit_max_xyz)
            )

    @property
    def angle_limit(self) -> tuple[_Vec3, _Vec3] | None:
        if self.angle_limit_min_xyz is None and self.angle_limit_max_xyz is None:
            return None
        return self.angle_limit_min_xyz, self.angle_limit_max_xyz  # type: ignore[return-value]


@dataclass(frozen=True)
class IkSolver:
    ik_bone: int
    target_bone: int
    links: tuple[IkLink, ...] = ()
    iteration_count: int = 1
    limit_angle: float = 0.0

    def __post_init__(self) -> None:
        object.__setattr__(self, "links", tuple(self.links))


@dataclass(frozen=True)
class AppendTransform:
    target_bone: int
    source_bone: int
    ratio: float
    affect_rotation: bool = False
    affect_translation: bool = False
    local: bool = False


@dataclass(frozen=True)
class BoneMorphOffset:
    morph_index: int
    target_bone: int
    position_offset_xyz: _Vec3
    rotation_offset_xyzw: _Vec4

    def __post_init__(self) -> None:
        object.__setattr__(self, "position_offset_xyz", tuple(self.position_offset_xyz))
        object.__setattr__(
            self, "rotation_offset_xyzw", tuple(self.rotation_offset_xyzw)
        )


@dataclass(frozen=True)
class GroupMorphOffset:
    morph_index: int
    child_morph: int
    ratio: float


@dataclass(frozen=True)
class ModelDefinition:
    """Immutable complete model snapshot consumed by the native constructor."""

    bones: tuple[Bone, ...]
    ik_solvers: tuple[IkSolver, ...] = ()
    append_transforms: tuple[AppendTransform, ...] = ()
    morph_count: int = 0
    bone_morph_offsets: tuple[BoneMorphOffset, ...] = ()
    group_morph_offsets: tuple[GroupMorphOffset, ...] = ()

    def __post_init__(self) -> None:
        object.__setattr__(self, "bones", tuple(self.bones))
        object.__setattr__(self, "ik_solvers", tuple(self.ik_solvers))
        object.__setattr__(self, "append_transforms", tuple(self.append_transforms))
        object.__setattr__(self, "bone_morph_offsets", tuple(self.bone_morph_offsets))
        object.__setattr__(self, "group_morph_offsets", tuple(self.group_morph_offsets))


class _ModelDescriptorCarrier:
    """Own all ctypes storage referenced by one ``ModelDescriptor``."""

    __slots__ = (
        "__weakref__",
        "bones",
        "ik_solvers",
        "ik_links",
        "append_transforms",
        "bone_morph_offsets",
        "group_morph_offsets",
        "descriptor",
    )

    def __init__(self, definition: ModelDefinition) -> None:
        self.bones = _marshal_bones(definition.bones)
        self.ik_solvers, self.ik_links = _marshal_ik(definition.ik_solvers)
        self.append_transforms = _marshal_appends(definition.append_transforms)
        self.bone_morph_offsets = _marshal_bone_morphs(definition.bone_morph_offsets)
        self.group_morph_offsets = _marshal_group_morphs(definition.group_morph_offsets)

        self.descriptor = ModelDescriptor(
            struct_size=ctypes.sizeof(ModelDescriptor),
            descriptor_version=DESCRIPTOR_VERSION,
            flags=MODEL_DESCRIPTOR_FLAGS_NONE,
            reserved=0,
            bones=self.bones,
            bone_count=_array_count(self.bones),
            ik_solvers=self.ik_solvers,
            ik_solver_count=_array_count(self.ik_solvers),
            ik_links=self.ik_links,
            ik_link_count=_array_count(self.ik_links),
            append_transforms=self.append_transforms,
            append_transform_count=_array_count(self.append_transforms),
            morph_count=_as_nonnegative_int(definition.morph_count, name="morph_count"),
            bone_morph_offsets=self.bone_morph_offsets,
            bone_morph_offset_count=_array_count(self.bone_morph_offsets),
            group_morph_offsets=self.group_morph_offsets,
            group_morph_offset_count=_array_count(self.group_morph_offsets),
        )


def _array_count(value: object | None) -> int:
    return 0 if value is None else len(value)  # type: ignore[arg-type]


def _optional_array(
    array_type: type[ctypes.Array], values: list[object]
) -> object | None:
    return None if not values else array_type(*values)


def _marshal_bones(values: tuple[Bone, ...]) -> object | None:
    records: list[ModelBoneDescriptor] = []
    for index, bone in enumerate(values):
        parent = bone.parent
        if parent is None:
            parent = -1
        if isinstance(parent, bool) or not isinstance(parent, int):
            raise TypeError(f"bones[{index}].parent must be an integer or None")
        if not -0x80000000 <= parent <= 0x7FFFFFFF:
            raise ValueError(f"bones[{index}].parent exceeds int32 range")
        rest = _as_tuple(
            bone.rest_position_pmx_xyz,
            name=f"bones[{index}].rest_position_pmx_xyz",
            length=3,
        )
        fixed = _as_tuple(
            bone.fixed_axis_xyz if bone.fixed_axis_xyz is not None else (0.0, 0.0, 0.0),
            name=f"bones[{index}].fixed_axis_xyz",
            length=3,
        )
        local_x_present = bone.local_axis_x_xyz is not None
        local_z_present = bone.local_axis_z_xyz is not None
        if local_x_present != local_z_present:
            raise ValueError(
                f"bones[{index}].local_axis_x_xyz and local_axis_z_xyz must be provided together"
            )
        local_x = _as_tuple(
            bone.local_axis_x_xyz if local_x_present else (0.0, 0.0, 0.0),
            name=f"bones[{index}].local_axis_x_xyz",
            length=3,
        )
        local_z = _as_tuple(
            bone.local_axis_z_xyz if local_z_present else (0.0, 0.0, 0.0),
            name=f"bones[{index}].local_axis_z_xyz",
            length=3,
        )
        after_physics = _as_bool(
            bone.transform_after_physics,
            name=f"bones[{index}].transform_after_physics",
        )
        flags = (
            (MODEL_BONE_TRANSFORM_AFTER_PHYSICS if after_physics else 0)
            | (MODEL_BONE_FIXED_AXIS if bone.fixed_axis_xyz is not None else 0)
            | (MODEL_BONE_LOCAL_AXIS if local_x_present else 0)
        )
        try:
            transform_order = int(bone.transform_order)
        except (TypeError, ValueError, OverflowError) as error:
            raise TypeError(
                f"bones[{index}].transform_order must be an integer"
            ) from error
        if isinstance(bone.transform_order, bool) or not isinstance(
            bone.transform_order, int
        ):
            raise TypeError(f"bones[{index}].transform_order must be an integer")
        if not -0x80000000 <= transform_order <= 0x7FFFFFFF:
            raise ValueError(f"bones[{index}].transform_order exceeds int32 range")
        records.append(
            ModelBoneDescriptor(
                parent_index=parent,
                rest_position_xyz=rest,
                transform_order=transform_order,
                flags=flags,
                fixed_axis_xyz=fixed,
                local_axis_x_xyz=local_x,
                local_axis_z_xyz=local_z,
            )
        )
    return _optional_array(ModelBoneDescriptor * len(records), records)


def _marshal_ik(
    values: tuple[IkSolver, ...],
) -> tuple[object | None, object | None]:
    solvers: list[ModelIkSolverDescriptor] = []
    links: list[ModelIkLinkDescriptor] = []
    for index, solver in enumerate(values):
        ik_bone = _as_nonnegative_int(
            solver.ik_bone, name=f"ik_solvers[{index}].ik_bone"
        )
        target_bone = _as_nonnegative_int(
            solver.target_bone, name=f"ik_solvers[{index}].target_bone"
        )
        iteration_count = _as_nonnegative_int(
            solver.iteration_count, name=f"ik_solvers[{index}].iteration_count"
        )
        link_offset = len(links)
        for link_index, link in enumerate(solver.links):
            bone = _as_nonnegative_int(
                link.bone, name=f"ik_solvers[{index}].links[{link_index}].bone"
            )
            min_present = link.angle_limit_min_xyz is not None
            max_present = link.angle_limit_max_xyz is not None
            if min_present != max_present:
                raise ValueError(
                    f"ik_solvers[{index}].links[{link_index}].angle_limit min/max must be paired"
                )
            minimum = _as_tuple(
                link.angle_limit_min_xyz if min_present else (0.0, 0.0, 0.0),
                name=f"ik_solvers[{index}].links[{link_index}].angle_limit_min_xyz",
                length=3,
            )
            maximum = _as_tuple(
                link.angle_limit_max_xyz if max_present else (0.0, 0.0, 0.0),
                name=f"ik_solvers[{index}].links[{link_index}].angle_limit_max_xyz",
                length=3,
            )
            links.append(
                ModelIkLinkDescriptor(
                    bone_index=bone,
                    flags=IK_LINK_ANGLE_LIMIT if min_present else 0,
                    angle_limit_min_xyz=minimum,
                    angle_limit_max_xyz=maximum,
                )
            )
        solvers.append(
            ModelIkSolverDescriptor(
                ik_bone_index=ik_bone,
                target_bone_index=target_bone,
                link_offset=link_offset,
                link_count=len(links) - link_offset,
                iteration_count=iteration_count,
                limit_angle=solver.limit_angle,
            )
        )
    return (
        _optional_array(ModelIkSolverDescriptor * len(solvers), solvers),
        _optional_array(ModelIkLinkDescriptor * len(links), links),
    )


def _marshal_appends(values: tuple[AppendTransform, ...]) -> object | None:
    records: list[ModelAppendDescriptor] = []
    for index, value in enumerate(values):
        target = _as_nonnegative_int(
            value.target_bone, name=f"append_transforms[{index}].target_bone"
        )
        source = _as_nonnegative_int(
            value.source_bone, name=f"append_transforms[{index}].source_bone"
        )
        rotation = _as_bool(
            value.affect_rotation, name=f"append_transforms[{index}].affect_rotation"
        )
        translation = _as_bool(
            value.affect_translation,
            name=f"append_transforms[{index}].affect_translation",
        )
        local = _as_bool(value.local, name=f"append_transforms[{index}].local")
        flags = (
            (APPEND_ROTATION if rotation else 0)
            | (APPEND_TRANSLATION if translation else 0)
            | (APPEND_LOCAL if local else 0)
        )
        records.append(ModelAppendDescriptor(target, source, value.ratio, flags))
    return _optional_array(ModelAppendDescriptor * len(records), records)


def _marshal_bone_morphs(values: tuple[BoneMorphOffset, ...]) -> object | None:
    records: list[ModelBoneMorphOffsetDescriptor] = []
    for index, value in enumerate(values):
        position = _as_tuple(
            value.position_offset_xyz,
            name=f"bone_morph_offsets[{index}].position_offset_xyz",
            length=3,
        )
        rotation = _as_tuple(
            value.rotation_offset_xyzw,
            name=f"bone_morph_offsets[{index}].rotation_offset_xyzw",
            length=4,
        )
        records.append(
            ModelBoneMorphOffsetDescriptor(
                morph_index=_as_nonnegative_int(
                    value.morph_index, name=f"bone_morph_offsets[{index}].morph_index"
                ),
                target_bone_index=_as_nonnegative_int(
                    value.target_bone, name=f"bone_morph_offsets[{index}].target_bone"
                ),
                position_offset_xyz=position,
                rotation_offset_xyzw=rotation,
            )
        )
    return _optional_array(ModelBoneMorphOffsetDescriptor * len(records), records)


def _marshal_group_morphs(values: tuple[GroupMorphOffset, ...]) -> object | None:
    records: list[ModelGroupMorphOffsetDescriptor] = []
    for index, value in enumerate(values):
        records.append(
            ModelGroupMorphOffsetDescriptor(
                morph_index=_as_nonnegative_int(
                    value.morph_index, name=f"group_morph_offsets[{index}].morph_index"
                ),
                child_morph_index=_as_nonnegative_int(
                    value.child_morph, name=f"group_morph_offsets[{index}].child_morph"
                ),
                ratio=value.ratio,
            )
        )
    return _optional_array(ModelGroupMorphOffsetDescriptor * len(records), records)


def marshal_model_definition(definition: ModelDefinition) -> _ModelDescriptorCarrier:
    """Build a carrier while retaining all pointed-to storage."""

    if not isinstance(definition, ModelDefinition):
        raise TypeError("definition must be a ModelDefinition")
    return _ModelDescriptorCarrier(definition)


__all__ = [
    "AppendTransform",
    "Bone",
    "BoneMorphOffset",
    "GroupMorphOffset",
    "IkLink",
    "IkSolver",
    "ModelDefinition",
    "marshal_model_definition",
]
