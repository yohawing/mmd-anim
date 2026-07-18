"""Host-neutral model snapshot reload orchestration for DCC integrations.

This module deliberately keeps the current pose on the host side.  A reload is
an edit-commit operation: it builds a complete candidate handle set, validates
and seeds an optional fresh physics world, then swaps the set atomically under
one lock.  Per-frame pose evaluation never rebuilds the model descriptor.
"""

from __future__ import annotations

import ctypes
import threading
import warnings
from dataclasses import dataclass
from typing import Iterable, TypeAlias

from ._abi import (
    HostPoseView,
    PhysicsJointDescriptor,
    PhysicsRigidbodyDescriptor,
    PhysicsWorldStepReport,
)
from ._runtime import Instance, Model, NativeRuntimeError, PhysicsWorld, RuntimeLibrary


_Vec3: TypeAlias = tuple[float, float, float]
_Vec4: TypeAlias = tuple[float, float, float, float]

PHYSICS_FRAME_ACTION_SEED = 0
PHYSICS_FRAME_ACTION_STEP = 1
PHYSICS_RIGIDBODY_SHAPE_SPHERE = 0
PHYSICS_RIGIDBODY_MODE_STATIC = 0
PHYSICS_RIGIDBODY_MODE_DYNAMIC = 1
PHYSICS_RIGIDBODY_MODE_DYNAMIC_BONE = 2
PHYSICS_JOINT_GENERIC_6DOF_SPRING = 0

_UNSET = object()


def _tuple(value: Iterable[float], *, name: str, length: int) -> tuple[float, ...]:
    try:
        result = tuple(value)
    except TypeError as error:
        raise TypeError(f"{name} must be iterable") from error
    if len(result) != length:
        raise ValueError(f"{name} must contain exactly {length} values")
    return result


def _flat(value: Iterable[float], *, name: str, stride: int) -> tuple[float, ...]:
    try:
        result = tuple(value)
    except TypeError as error:
        raise TypeError(f"{name} must be iterable") from error
    if len(result) % stride:
        raise ValueError(f"{name} length must be a multiple of {stride}")
    return result


def _uint(value: object, *, name: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int):
        raise TypeError(f"{name} must be an integer")
    if not 0 <= value <= 0xFFFFFFFF:
        raise ValueError(f"{name} must fit uint32")
    return value


def _size(value: object, *, name: str) -> int:
    if isinstance(value, bool) or not isinstance(value, int):
        raise TypeError(f"{name} must be an integer")
    if value < 0:
        raise ValueError(f"{name} must be non-negative")
    return value


@dataclass(frozen=True)
class HostPose:
    """Owned host pose snapshot; no native pointer is retained."""

    local_position_offsets_xyz: tuple[float, ...] = ()
    local_rotation_xyzw: tuple[float, ...] = ()
    local_scales_xyz: tuple[float, ...] = ()
    morph_weights: tuple[float, ...] = ()
    ik_enabled: tuple[bool, ...] = ()

    def __post_init__(self) -> None:
        object.__setattr__(
            self,
            "local_position_offsets_xyz",
            tuple(self.local_position_offsets_xyz),
        )
        object.__setattr__(self, "local_rotation_xyzw", tuple(self.local_rotation_xyzw))
        object.__setattr__(self, "local_scales_xyz", tuple(self.local_scales_xyz))
        object.__setattr__(self, "morph_weights", tuple(self.morph_weights))
        object.__setattr__(self, "ik_enabled", tuple(self.ik_enabled))

    @classmethod
    def for_model(
        cls, bone_count: int, morph_count: int = 0, ik_count: int = 0
    ) -> HostPose:
        bone_count = _size(bone_count, name="bone_count")
        morph_count = _size(morph_count, name="morph_count")
        ik_count = _size(ik_count, name="ik_count")
        return cls(
            local_position_offsets_xyz=(0.0, 0.0, 0.0) * bone_count,
            local_rotation_xyzw=(0.0, 0.0, 0.0, 1.0) * bone_count,
            local_scales_xyz=(1.0, 1.0, 1.0) * bone_count,
            morph_weights=(0.0,) * morph_count,
            ik_enabled=(True,) * ik_count,
        )


@dataclass(frozen=True)
class PhysicsRigidbody:
    shape: int = PHYSICS_RIGIDBODY_SHAPE_SPHERE
    shape_size: _Vec3 = (1.0, 1.0, 1.0)
    position_xyz: _Vec3 = (0.0, 0.0, 0.0)
    rotation_euler_xyz: _Vec3 = (0.0, 0.0, 0.0)
    mass: float = 1.0
    linear_damping: float = 0.0
    angular_damping: float = 0.0
    friction: float = 0.5
    restitution: float = 0.0
    collision_group: int = 0xFFFF
    collision_mask: int = 0xFFFF
    bone_index: int = -1
    mode: int = PHYSICS_RIGIDBODY_MODE_STATIC
    body_from_bone_position_xyz: _Vec3 = (0.0, 0.0, 0.0)
    body_from_bone_rotation_xyzw: _Vec4 = (0.0, 0.0, 0.0, 1.0)
    bone_from_body_position_xyz: _Vec3 = (0.0, 0.0, 0.0)
    bone_from_body_rotation_xyzw: _Vec4 = (0.0, 0.0, 0.0, 1.0)

    def __post_init__(self) -> None:
        for name, value, length in (
            ("shape_size", self.shape_size, 3),
            ("position_xyz", self.position_xyz, 3),
            ("rotation_euler_xyz", self.rotation_euler_xyz, 3),
            ("body_from_bone_position_xyz", self.body_from_bone_position_xyz, 3),
            ("body_from_bone_rotation_xyzw", self.body_from_bone_rotation_xyzw, 4),
            ("bone_from_body_position_xyz", self.bone_from_body_position_xyz, 3),
            ("bone_from_body_rotation_xyzw", self.bone_from_body_rotation_xyzw, 4),
        ):
            object.__setattr__(self, name, tuple(value))


@dataclass(frozen=True)
class PhysicsJoint:
    kind: int = PHYSICS_JOINT_GENERIC_6DOF_SPRING
    rigidbody_a: int = 0
    rigidbody_b: int = 0
    position_xyz: _Vec3 = (0.0, 0.0, 0.0)
    rotation_euler_xyz: _Vec3 = (0.0, 0.0, 0.0)
    translation_lower_limit_xyz: _Vec3 = (0.0, 0.0, 0.0)
    translation_upper_limit_xyz: _Vec3 = (0.0, 0.0, 0.0)
    rotation_lower_limit_xyz: _Vec3 = (0.0, 0.0, 0.0)
    rotation_upper_limit_xyz: _Vec3 = (0.0, 0.0, 0.0)
    spring_translation_factor_xyz: _Vec3 = (0.0, 0.0, 0.0)
    spring_rotation_factor_xyz: _Vec3 = (0.0, 0.0, 0.0)

    def __post_init__(self) -> None:
        for name in (
            "position_xyz",
            "rotation_euler_xyz",
            "translation_lower_limit_xyz",
            "translation_upper_limit_xyz",
            "rotation_lower_limit_xyz",
            "rotation_upper_limit_xyz",
            "spring_translation_factor_xyz",
            "spring_rotation_factor_xyz",
        ):
            object.__setattr__(self, name, tuple(getattr(self, name)))


@dataclass(frozen=True)
class PhysicsDefinition:
    rigidbodies: tuple[PhysicsRigidbody, ...] = ()
    joints: tuple[PhysicsJoint, ...] = ()

    def __post_init__(self) -> None:
        object.__setattr__(self, "rigidbodies", tuple(self.rigidbodies))
        object.__setattr__(self, "joints", tuple(self.joints))


@dataclass(frozen=True)
class _HostPoseCarrier:
    positions: object | None
    rotations: object | None
    scales: object | None
    morphs: object | None
    ik_enabled: object | None
    view: HostPoseView


def marshal_host_pose(pose: HostPose) -> _HostPoseCarrier:
    if not isinstance(pose, HostPose):
        raise TypeError("pose must be a HostPose")
    positions = _flat(
        pose.local_position_offsets_xyz, name="local_position_offsets_xyz", stride=3
    )
    rotations = _flat(pose.local_rotation_xyzw, name="local_rotation_xyzw", stride=4)
    scales = _flat(pose.local_scales_xyz, name="local_scales_xyz", stride=3)
    bone_counts = {len(positions) // 3, len(rotations) // 4, len(scales) // 3}
    if len(bone_counts) != 1:
        raise ValueError("host pose bone arrays must describe the same bone count")
    for index, enabled in enumerate(pose.ik_enabled):
        if type(enabled) is not bool:
            raise TypeError(f"ik_enabled[{index}] must be bool")
    positions_array = (
        (ctypes.c_float * len(positions))(*positions) if positions else None
    )
    rotations_array = (
        (ctypes.c_float * len(rotations))(*rotations) if rotations else None
    )
    scales_array = (ctypes.c_float * len(scales))(*scales) if scales else None
    morphs_array = (
        (ctypes.c_float * len(pose.morph_weights))(*pose.morph_weights)
        if pose.morph_weights
        else None
    )
    ik_array = (
        (ctypes.c_uint8 * len(pose.ik_enabled))(
            *(1 if value else 0 for value in pose.ik_enabled)
        )
        if pose.ik_enabled
        else None
    )
    return _HostPoseCarrier(
        positions_array,
        rotations_array,
        scales_array,
        morphs_array,
        ik_array,
        HostPoseView(
            local_position_offsets_xyz=positions_array,
            local_rotation_xyzw=rotations_array,
            local_scales_xyz=scales_array,
            bone_count=len(positions) // 3,
            morph_weights=morphs_array,
            morph_count=len(pose.morph_weights),
            ik_enabled=ik_array,
            ik_count=len(pose.ik_enabled),
        ),
    )


@dataclass(frozen=True)
class _PhysicsDescriptorCarrier:
    rigidbodies: object | None
    joints: object | None


def marshal_physics_definition(
    definition: PhysicsDefinition,
) -> _PhysicsDescriptorCarrier:
    if not isinstance(definition, PhysicsDefinition):
        raise TypeError("physics_definition must be a PhysicsDefinition")
    rigidbodies: list[PhysicsRigidbodyDescriptor] = []
    for index, body in enumerate(definition.rigidbodies):
        shape_size = _tuple(
            body.shape_size, name=f"rigidbodies[{index}].shape_size", length=3
        )
        position = _tuple(
            body.position_xyz, name=f"rigidbodies[{index}].position_xyz", length=3
        )
        rotation = _tuple(
            body.rotation_euler_xyz,
            name=f"rigidbodies[{index}].rotation_euler_xyz",
            length=3,
        )
        body_pos = _tuple(
            body.body_from_bone_position_xyz,
            name=f"rigidbodies[{index}].body_from_bone_position_xyz",
            length=3,
        )
        body_rot = _tuple(
            body.body_from_bone_rotation_xyzw,
            name=f"rigidbodies[{index}].body_from_bone_rotation_xyzw",
            length=4,
        )
        bone_pos = _tuple(
            body.bone_from_body_position_xyz,
            name=f"rigidbodies[{index}].bone_from_body_position_xyz",
            length=3,
        )
        bone_rot = _tuple(
            body.bone_from_body_rotation_xyzw,
            name=f"rigidbodies[{index}].bone_from_body_rotation_xyzw",
            length=4,
        )
        if not -0x80000000 <= body.bone_index <= 0x7FFFFFFF:
            raise ValueError(f"rigidbodies[{index}].bone_index must fit int32")
        for name, value in (("shape", body.shape), ("mode", body.mode)):
            _uint(value, name=f"rigidbodies[{index}].{name}")
        for name, value in (
            ("collision_group", body.collision_group),
            ("collision_mask", body.collision_mask),
        ):
            if (
                isinstance(value, bool)
                or not isinstance(value, int)
                or not 0 <= value <= 0xFFFF
            ):
                raise ValueError(f"rigidbodies[{index}].{name} must fit uint16")
        rigidbodies.append(
            PhysicsRigidbodyDescriptor(
                body.shape,
                shape_size,
                position,
                rotation,
                body.mass,
                body.linear_damping,
                body.angular_damping,
                body.friction,
                body.restitution,
                body.collision_group,
                body.collision_mask,
                body.bone_index,
                body.mode,
                body_pos,
                body_rot,
                bone_pos,
                bone_rot,
            )
        )
    joints: list[PhysicsJointDescriptor] = []
    for index, joint in enumerate(definition.joints):
        fields = [
            _tuple(getattr(joint, name), name=f"joints[{index}].{name}", length=3)
            for name in (
                "position_xyz",
                "rotation_euler_xyz",
                "translation_lower_limit_xyz",
                "translation_upper_limit_xyz",
                "rotation_lower_limit_xyz",
                "rotation_upper_limit_xyz",
                "spring_translation_factor_xyz",
                "spring_rotation_factor_xyz",
            )
        ]
        joints.append(
            PhysicsJointDescriptor(
                _uint(joint.kind, name=f"joints[{index}].kind"),
                _size(joint.rigidbody_a, name=f"joints[{index}].rigidbody_a"),
                _size(joint.rigidbody_b, name=f"joints[{index}].rigidbody_b"),
                *fields,
            )
        )
    return _PhysicsDescriptorCarrier(
        (PhysicsRigidbodyDescriptor * len(rigidbodies))(*rigidbodies)
        if rigidbodies
        else None,
        (PhysicsJointDescriptor * len(joints))(*joints) if joints else None,
    )


@dataclass(frozen=True)
class RuntimeHandleSet:
    """One owned model/instance/world generation."""

    model: Model
    instance: Instance
    physics_world: PhysicsWorld | None = None

    def close(self) -> None:
        first_error: Exception | None = None
        for handle in (self.physics_world, self.instance, self.model):
            if handle is None:
                continue
            try:
                handle.close()
            except Exception as error:  # pragma: no cover - defensive fake cleanup
                if first_error is None:
                    first_error = error
        if first_error is not None:
            raise first_error


class LiveRuntime:
    """Host-facing fail-atomic model reload and per-frame evaluation wrapper."""

    def __init__(
        self,
        runtime: RuntimeLibrary,
        definition: object | None = None,
        current_pose: HostPose | None = None,
        physics_definition: PhysicsDefinition | None = None,
    ) -> None:
        self._runtime = runtime
        self._lock = threading.RLock()
        self._handles: RuntimeHandleSet | None = None
        self._closed = False
        if definition is not None:
            if current_pose is None:
                raise ValueError("current_pose is required for initial model creation")
            self.reload(definition, current_pose, physics_definition)

    @property
    def handle_set(self) -> RuntimeHandleSet:
        with self._lock:
            if self._handles is None:
                raise NativeRuntimeError("live runtime has no active handle set")
            return self._handles

    @property
    def model(self) -> Model:
        return self.handle_set.model

    @property
    def instance(self) -> Instance:
        return self.handle_set.instance

    @property
    def physics_world(self) -> PhysicsWorld | None:
        return self.handle_set.physics_world

    def _cleanup_candidate(
        self,
        world: PhysicsWorld | None,
        instance: Instance | None,
        model: Model | None,
    ) -> None:
        first_error: Exception | None = None
        for handle in (world, instance, model):
            if handle is None:
                continue
            try:
                handle.close()
            except Exception as error:
                if first_error is None:
                    first_error = error
        # Candidate cleanup must never mask the construction failure.
        del first_error

    def reload(
        self,
        definition: object,
        current_pose: HostPose,
        physics_definition: PhysicsDefinition | None | object = _UNSET,
    ) -> None:
        with self._lock:
            if self._closed:
                raise NativeRuntimeError("live runtime is closed")
            if not isinstance(current_pose, HostPose):
                raise TypeError("current_pose must be a HostPose")
            previous_physics = (
                self._physics_definition
                if hasattr(self, "_physics_definition")
                else None
            )
            selected_physics = (
                previous_physics if physics_definition is _UNSET else physics_definition
            )
            if selected_physics is not None and not isinstance(
                selected_physics, PhysicsDefinition
            ):
                raise TypeError(
                    "physics_definition must be a PhysicsDefinition or None"
                )

            model: Model | None = None
            instance: Instance | None = None
            world: PhysicsWorld | None = None
            try:
                model = self._runtime.create_model_from_descriptor(definition)  # type: ignore[arg-type]
                instance = model.create_instance_for_model()
                if selected_physics is None:
                    instance.apply_host_pose_and_evaluate_before_physics(current_pose)
                    instance.evaluate_current_pose_after_physics()
                else:
                    # Apply the DCC's committed pose before constructing the fresh
                    # world; the SEED call below repeats the validated view as one
                    # atomic physics/evaluation operation.
                    instance.apply_host_pose(current_pose)
                    world = self._runtime.create_physics_world_from_descriptors(
                        selected_physics
                    )
                    self._runtime.evaluate_host_frame(
                        instance,
                        world,
                        current_pose,
                        action=PHYSICS_FRAME_ACTION_SEED,
                        dt_seconds=0.0,
                    )
                candidate = RuntimeHandleSet(model, instance, world)
            except Exception:
                self._cleanup_candidate(world, instance, model)
                raise

            old = self._handles
            self._handles = candidate
            self._physics_definition = selected_physics
            if old is not None:
                try:
                    old.close()
                except Exception as error:
                    warnings.warn(
                        f"live runtime old handle cleanup failed after swap: {error}",
                        ResourceWarning,
                        stacklevel=2,
                    )

    def evaluate_host_frame(
        self,
        pose: HostPose,
        *,
        action: int = PHYSICS_FRAME_ACTION_STEP,
        dt_seconds: float = 1.0 / 60.0,
    ) -> PhysicsWorldStepReport | None:
        with self._lock:
            handles = self.handle_set
            if handles.physics_world is None:
                handles.instance.apply_host_pose_and_evaluate_before_physics(pose)
                handles.instance.evaluate_current_pose_after_physics()
                return None
            return self._runtime.evaluate_host_frame(
                handles.instance,
                handles.physics_world,
                pose,
                action=action,
                dt_seconds=dt_seconds,
            )

    apply_host_pose = evaluate_host_frame

    def close(self) -> None:
        with self._lock:
            if self._closed:
                return
            self._closed = True
            old = self._handles
            self._handles = None
            self._physics_definition = None
            if old is not None:
                try:
                    old.close()
                except Exception as error:
                    warnings.warn(
                        f"live runtime cleanup failed during close: {error}",
                        ResourceWarning,
                        stacklevel=2,
                    )


__all__ = [
    "HostPose",
    "LiveRuntime",
    "PHYSICS_FRAME_ACTION_SEED",
    "PHYSICS_FRAME_ACTION_STEP",
    "PHYSICS_JOINT_GENERIC_6DOF_SPRING",
    "PHYSICS_RIGIDBODY_MODE_DYNAMIC",
    "PHYSICS_RIGIDBODY_MODE_DYNAMIC_BONE",
    "PHYSICS_RIGIDBODY_MODE_STATIC",
    "PHYSICS_RIGIDBODY_SHAPE_SPHERE",
    "PhysicsDefinition",
    "PhysicsJoint",
    "PhysicsRigidbody",
    "RuntimeHandleSet",
    "marshal_host_pose",
    "marshal_physics_definition",
]
