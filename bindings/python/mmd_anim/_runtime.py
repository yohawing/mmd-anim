"""Minimal, internal ctypes binding for the mmd-anim runtime C ABI v2."""

from __future__ import annotations

import ctypes
import json
import os
import sys
from array import array
from collections.abc import Mapping
from pathlib import Path
from typing import Callable, Sequence

from ._abi import (
    ByteBuffer,
    IkSolveStats,
    PhysicsRigidbodyBinding,
    PhysicsWorldStepReport,
    RigBone,
    RigIkLink,
    bind_functions,
)
from ._model_descriptor import (
    FEATURE_MODEL_DESCRIPTOR,
    ModelDefinition,
    marshal_model_definition,
)


EXPECTED_ABI_VERSION = 2
FEATURE_PHYSICS_BULLET_NATIVE = 1 << 1
FEATURE_HOST_POSE_NATIVE_MORPHS = 1 << 3
LIBRARY_ENV = "MMD_RUNTIME_LIBRARY"


class NativeRuntimeError(RuntimeError):
    """Raised when loading or calling the native runtime fails."""


_STATUS_NAMES = {
    0: "OK",
    1: "INVALID_INPUT",
    2: "UNSUPPORTED",
    3: "BUFFER_TOO_SMALL",
    4: "ERROR",
}


class NativeStatusError(NativeRuntimeError):
    """Raised when a status-returning native function does not return OK."""

    def __init__(self, operation: str, status: int, detail: str | None) -> None:
        self.operation = operation
        self.status = int(status)
        self.status_name = _STATUS_NAMES.get(self.status, "UNKNOWN_STATUS")
        self.detail = detail
        message = f"{operation} failed with {self.status_name} ({self.status})"
        if detail:
            message += f": {detail}"
        super().__init__(message)


class AbiVersionError(NativeRuntimeError):
    """Raised when the loaded native library does not implement ABI v2."""


def _require_abi_version(actual: int) -> None:
    if actual != EXPECTED_ABI_VERSION:
        raise AbiVersionError(
            f"mmd-anim runtime ABI mismatch: expected {EXPECTED_ABI_VERSION}, got {actual}"
        )


def _library_filename() -> str:
    if sys.platform == "win32":
        return "mmd_runtime_ffi.dll"
    if sys.platform == "darwin":
        return "libmmd_runtime_ffi.dylib"
    return "libmmd_runtime_ffi.so"


def local_release_library_path() -> Path:
    repository_root = Path(__file__).resolve().parents[3]
    return repository_root / "target" / "release" / _library_filename()


def resolve_library_path(path: os.PathLike[str] | str | None = None) -> Path:
    """Resolve an explicit path, the environment override, or local release output."""

    configured = path if path is not None else os.environ.get(LIBRARY_ENV)
    candidate = (
        Path(configured).expanduser()
        if configured is not None
        else local_release_library_path()
    )
    if not candidate.is_file():
        source = "configured" if configured is not None else "local release"
        raise NativeRuntimeError(
            f"{source} mmd-anim runtime library was not found: {candidate}"
        )
    return candidate.resolve()


def _copy_and_free_byte_buffer(
    buffer: ByteBuffer, free_buffer: Callable[[ByteBuffer], None]
) -> bytes:
    """Copy an owned ABI buffer, then free it exactly once even on failure."""

    try:
        if buffer.len == 0:
            return b""
        if not buffer.data:
            raise NativeRuntimeError("native byte buffer has non-zero length and NULL data")
        return ctypes.string_at(buffer.data, buffer.len)
    finally:
        free_buffer(buffer)


class RuntimeLibrary:
    """Loaded ABI v2 function table; internal and experimental."""

    def __init__(self, path: os.PathLike[str] | str | None = None) -> None:
        self.path = resolve_library_path(path)
        try:
            self._lib = ctypes.CDLL(str(self.path))
        except OSError as error:
            raise NativeRuntimeError(
                f"failed to load mmd-anim runtime library {self.path}: {error}"
            ) from error
        try:
            self._bind()
        except AttributeError as error:
            raise NativeRuntimeError(
                f"mmd-anim runtime library is missing a required ABI v2 symbol: {error}"
            ) from error

    def _bind(self) -> None:
        lib = self._lib
        bind_functions(lib)
        _require_abi_version(lib.mmd_runtime_abi_version())

    def feature_flags(self) -> int:
        return int(self._lib.mmd_runtime_feature_flags())

    def supports_native_physics(self) -> bool:
        return bool(self.feature_flags() & FEATURE_PHYSICS_BULLET_NATIVE)

    def supports_native_host_pose_morphs(self) -> bool:
        """Return whether HostPose weights expand Group/Bone Morphs natively."""

        return bool(self.feature_flags() & FEATURE_HOST_POSE_NATIVE_MORPHS)

    def supports_model_descriptor(self) -> bool:
        """Return whether the optional typed descriptor constructor is usable."""

        return bool(
            self.feature_flags() & FEATURE_MODEL_DESCRIPTOR
            and hasattr(self._lib, "mmd_runtime_model_create_from_descriptor")
        )

    def _last_error(self) -> str | None:
        value = self._lib.mmd_runtime_last_error_message()
        if not value:
            return None
        # c_char_p copies the borrowed C string to bytes before returning. Keep
        # integer-pointer handling for lightweight fake libraries in pure tests.
        copied = value if isinstance(value, bytes) else ctypes.string_at(value)
        return copied.decode("utf-8", errors="replace")

    def _failure(self, operation: str) -> NativeRuntimeError:
        detail = self._last_error()
        return NativeRuntimeError(f"{operation} failed" + (f": {detail}" if detail else ""))

    def _require_ok(self, status: int, operation: str) -> None:
        numeric_status = int(status)
        if numeric_status != 0:
            raise NativeStatusError(operation, numeric_status, self._last_error())

    def create_model(
        self, parent_indices: Sequence[int], rest_positions_xyz: Sequence[float]
    ) -> Model:
        bone_count = len(parent_indices)
        if bone_count == 0:
            raise ValueError("parent_indices must contain at least one bone")
        if len(rest_positions_xyz) != bone_count * 3:
            raise ValueError(
                "rest_positions_xyz must contain exactly three floats per bone"
            )

        parents = (ctypes.c_int32 * bone_count)(*parent_indices)
        rest = (ctypes.c_float * (bone_count * 3))(*rest_positions_xyz)
        handle = self._lib.mmd_runtime_model_create(parents, rest, bone_count)
        if not handle:
            raise self._failure("mmd_runtime_model_create")
        return Model(self, handle)

    def create_model_from_pmx_bytes(self, data: bytes) -> Model:
        storage, length = self._input_bytes(data)
        handle = self._lib.mmd_runtime_model_create_from_pmx_bytes(storage, length)
        if not handle:
            raise self._failure("mmd_runtime_model_create_from_pmx_bytes")
        return Model(self, handle)

    def create_model_from_descriptor(self, definition: ModelDefinition) -> Model:
        """Create a model from an immutable, payload-free typed snapshot.

        The carrier is deliberately kept in a local until the native call
        returns.  The ABI constructor copies all pointed-to records before it
        returns, so no Python storage is retained by the resulting ``Model``.
        """

        if not self.supports_model_descriptor():
            raise NativeRuntimeError(
                "mmd_runtime_model_create_from_descriptor is unsupported by this runtime"
            )
        carrier = marshal_model_definition(definition)
        handle = self._lib.mmd_runtime_model_create_from_descriptor(
            ctypes.byref(carrier.descriptor)
        )
        if not handle:
            # Read the indexed TLS error immediately, before any other native
            # operation can overwrite it.
            raise self._failure("mmd_runtime_model_create_from_descriptor")
        return Model(self, handle)

    def create_physics_world_from_pmx_bytes(self, data: bytes) -> PhysicsWorld:
        storage, length = self._input_bytes(data)
        out_world = ctypes.c_void_p()
        operation = "mmd_runtime_physics_world_create_from_pmx_bytes"
        status = self._lib.mmd_runtime_physics_world_create_from_pmx_bytes(
            storage, length, ctypes.byref(out_world)
        )
        try:
            self._require_ok(status, operation)
        except NativeStatusError:
            if out_world.value:
                self._lib.mmd_runtime_physics_world_free(out_world.value)
            raise
        if not out_world.value:
            raise NativeRuntimeError(f"{operation} returned OK with a NULL world")
        return PhysicsWorld(self, out_world.value)

    def create_physics_world_from_descriptors(self, definition: object) -> PhysicsWorld:
        """Create a typed physics world, copying descriptor arrays on return."""

        from ._live_reload import marshal_physics_definition

        carrier = marshal_physics_definition(definition)  # type: ignore[arg-type]
        out_world = ctypes.c_void_p()
        operation = "mmd_runtime_physics_world_create"
        status = self._lib.mmd_runtime_physics_world_create(
            carrier.rigidbodies,
            0 if carrier.rigidbodies is None else len(carrier.rigidbodies),
            carrier.joints,
            0 if carrier.joints is None else len(carrier.joints),
            ctypes.byref(out_world),
        )
        try:
            self._require_ok(status, operation)
        except NativeStatusError:
            if out_world.value:
                self._lib.mmd_runtime_physics_world_free(out_world.value)
            raise
        if not out_world.value:
            raise NativeRuntimeError(f"{operation} returned OK with a NULL world")
        return PhysicsWorld(self, out_world.value)

    def evaluate_host_frame(
        self,
        instance: Instance,
        world: PhysicsWorld,
        pose: object,
        *,
        action: int,
        dt_seconds: float,
        ik_tolerance: float = 1.0e-3,
        ik_max_iterations_cap: int = 0,
    ) -> PhysicsWorldStepReport:
        if instance._model._runtime is not self or world._runtime is not self:
            raise ValueError("instance, physics world, and runtime must match")
        from ._live_reload import marshal_host_pose

        carrier = marshal_host_pose(pose)  # type: ignore[arg-type]
        report = PhysicsWorldStepReport()
        status = self._lib.mmd_runtime_evaluate_host_frame(
            instance._require_open(),
            world._require_open(),
            ctypes.byref(carrier.view),
            action,
            dt_seconds,
            ik_tolerance,
            ik_max_iterations_cap,
            ctypes.byref(report),
        )
        self._require_ok(status, "mmd_runtime_evaluate_host_frame")
        return report

    def create_clip_from_vmd_bytes(self, model: Model, data: bytes) -> Clip:
        if model._runtime is not self:
            raise ValueError("model and clip must belong to the same RuntimeLibrary")
        model_handle = model._require_open()
        storage, length = self._input_bytes(data)
        handle = self._lib.mmd_runtime_clip_create_from_vmd_bytes_for_model(
            model_handle, storage, length
        )
        if not handle:
            raise self._failure(
                "mmd_runtime_clip_create_from_vmd_bytes_for_model"
            )
        return Clip(self, handle)

    def create_empty_clip(self) -> Clip:
        handle = self._lib.mmd_runtime_clip_create(
            None,
            0,
            None,
            0,
            None,
            0,
            None,
            0,
            None,
            0,
            None,
            0,
        )
        if not handle:
            raise self._failure("mmd_runtime_clip_create")
        return Clip(self, handle)

    @staticmethod
    def _input_bytes(data: bytes) -> tuple[object, int]:
        if not data:
            raise ValueError("native parser input must not be empty")
        storage = (ctypes.c_uint8 * len(data)).from_buffer_copy(data)
        return storage, len(data)

    def parse_vmd_json(self, data: bytes) -> object:
        storage, length = self._input_bytes(data)
        return self.decode_owned_json(
            self._lib.mmd_runtime_parse_vmd_json(storage, length),
            "mmd_runtime_parse_vmd_json",
        )

    def parse_pmx_non_geometry_json(self, data: bytes) -> object:
        storage, length = self._input_bytes(data)
        return self.decode_owned_json(
            self._lib.mmd_runtime_parse_pmx_non_geometry_json(storage, length),
            "mmd_runtime_parse_pmx_non_geometry_json",
        )

    def create_pmx_geometry(self, data: bytes) -> PmxGeometry:
        storage, length = self._input_bytes(data)
        handle = self._lib.mmd_runtime_pmx_geometry_create(storage, length)
        if not handle:
            raise self._failure("mmd_runtime_pmx_geometry_create")
        return PmxGeometry(self, handle)

    def create_ik_chain(
        self,
        bones: Sequence[RigBone],
        target_bone_slot: int,
        links: Sequence[RigIkLink],
        iteration_count: int,
        limit_angle: float,
    ) -> IkChain:
        if not bones or not links:
            raise ValueError("bones and links must not be empty")
        native_bones = (RigBone * len(bones))(*bones)
        native_links = (RigIkLink * len(links))(*links)
        handle = self._lib.mmd_runtime_ik_chain_create(
            native_bones,
            len(bones),
            target_bone_slot,
            native_links,
            len(links),
            iteration_count,
            limit_angle,
        )
        if not handle:
            raise self._failure("mmd_runtime_ik_chain_create")
        return IkChain(self, handle, len(bones), len(links))

    def copy_owned_bytes(self, buffer: ByteBuffer, operation: str) -> bytes:
        if buffer.len == 0 or not buffer.data:
            detail = self._last_error()
            self._lib.mmd_runtime_byte_buffer_free(buffer)
            raise NativeRuntimeError(
                f"{operation} returned an empty byte buffer"
                + (f": {detail}" if detail else "")
            )
        return _copy_and_free_byte_buffer(
            buffer, self._lib.mmd_runtime_byte_buffer_free
        )

    def decode_owned_json(self, buffer: ByteBuffer, operation: str) -> object:
        copied = self.copy_owned_bytes(buffer, operation)
        return json.loads(copied.decode("utf-8", errors="strict"))


class _OwnedHandle:
    """Shared context-manager/open-state behavior for opaque handles."""

    _closed_handle_message = "native handle is closed"

    def __enter__(self):
        self._require_open()
        return self

    def __exit__(self, exc_type: object, exc: object, traceback: object) -> None:
        self.close()

    def _require_open(self) -> int:
        if self._handle is None:
            raise NativeRuntimeError(self._closed_handle_message)
        return self._handle


class Model(_OwnedHandle):
    """Owned opaque model handle."""

    def __init__(self, runtime: RuntimeLibrary, handle: int) -> None:
        self._runtime = runtime
        self._handle: int | None = handle
        self._instances: set[Instance] = set()

    _closed_handle_message = "model handle is closed"

    def create_instance(self, morph_count: int = 0) -> Instance:
        if morph_count < 0:
            raise ValueError("morph_count must be non-negative")
        handle = self._runtime._lib.mmd_runtime_instance_create(
            self._require_open(), morph_count
        )
        if not handle:
            raise self._runtime._failure("mmd_runtime_instance_create")
        instance = Instance(self, handle)
        self._instances.add(instance)
        return instance

    def bone_count(self) -> int:
        return int(
            self._runtime._lib.mmd_runtime_model_bone_count(self._require_open())
        )

    def morph_count(self) -> int:
        return int(
            self._runtime._lib.mmd_runtime_model_morph_count(self._require_open())
        )

    def ik_count(self) -> int:
        return int(self._runtime._lib.mmd_runtime_model_ik_count(self._require_open()))

    def create_instance_for_model(self) -> Instance:
        handle = self._runtime._lib.mmd_runtime_instance_create_for_model(
            self._require_open()
        )
        if not handle:
            raise self._runtime._failure("mmd_runtime_instance_create_for_model")
        instance = Instance(self, handle)
        self._instances.add(instance)
        return instance

    def close(self) -> None:
        if self._handle is None:
            return
        for instance in tuple(self._instances):
            instance.close()
        self._runtime._lib.mmd_runtime_model_free(self._handle)
        self._handle = None


class Instance(_OwnedHandle):
    """Owned opaque runtime-instance handle."""

    def __init__(self, model: Model, handle: int) -> None:
        self._model = model
        self._handle: int | None = handle

    _closed_handle_message = "instance handle is closed"

    def evaluate_rest_pose(self) -> None:
        if not self._model._runtime._lib.mmd_runtime_instance_evaluate_rest_pose(
            self._require_open()
        ):
            raise self._model._runtime._failure(
                "mmd_runtime_instance_evaluate_rest_pose"
            )

    def apply_host_pose(self, pose: object) -> None:
        from ._live_reload import marshal_host_pose

        carrier = marshal_host_pose(pose)  # type: ignore[arg-type]
        status = self._model._runtime._lib.mmd_runtime_instance_apply_host_pose(
            self._require_open(), ctypes.byref(carrier.view)
        )
        self._model._runtime._require_ok(status, "mmd_runtime_instance_apply_host_pose")

    def apply_host_pose_and_evaluate_before_physics(self, pose: object) -> None:
        from ._live_reload import marshal_host_pose

        carrier = marshal_host_pose(pose)  # type: ignore[arg-type]
        status = self._model._runtime._lib.mmd_runtime_instance_apply_host_pose_and_evaluate_before_physics(
            self._require_open(), ctypes.byref(carrier.view)
        )
        self._model._runtime._require_ok(
            status,
            "mmd_runtime_instance_apply_host_pose_and_evaluate_before_physics",
        )

    def evaluate_current_pose_after_physics(self) -> None:
        status = self._model._runtime._lib.mmd_runtime_instance_evaluate_current_pose_after_physics(
            self._require_open()
        )
        self._model._runtime._require_ok(
            status, "mmd_runtime_instance_evaluate_current_pose_after_physics"
        )

    def evaluate_clip_frame(self, clip: Clip, frame: float) -> None:
        if clip._runtime is not self._model._runtime:
            raise ValueError("clip and instance must belong to the same RuntimeLibrary")
        if not self._model._runtime._lib.mmd_runtime_instance_evaluate_clip_frame(
            self._require_open(), clip._require_open(), frame
        ):
            raise self._model._runtime._failure(
                "mmd_runtime_instance_evaluate_clip_frame"
            )

    def _copy_f32_array(
        self,
        length_function: str,
        copy_function: str,
        *,
        allow_empty: bool,
    ) -> array:
        handle = self._require_open()
        library = self._model._runtime._lib
        length = int(getattr(library, length_function)(handle))
        if length == 0:
            if allow_empty:
                return array("f")
            raise self._model._runtime._failure(
                length_function
            )
        output = array("f", (0.0,)) * length
        native_output = (ctypes.c_float * length).from_buffer(output)
        if not getattr(library, copy_function)(handle, native_output, length):
            raise self._model._runtime._failure(copy_function)
        return output

    def world_matrices_f32(self) -> array:
        return self._copy_f32_array(
            "mmd_runtime_instance_world_matrix_f32_len",
            "mmd_runtime_instance_copy_world_matrices",
            allow_empty=False,
        )

    def skinning_matrices_f32(self) -> array:
        return self._copy_f32_array(
            "mmd_runtime_instance_skinning_matrix_f32_len",
            "mmd_runtime_instance_copy_skinning_matrices",
            allow_empty=False,
        )

    def morph_weights_f32(self) -> array:
        return self._copy_f32_array(
            "mmd_runtime_instance_morph_weight_len",
            "mmd_runtime_instance_copy_morph_weights",
            allow_empty=True,
        )

    def close(self) -> None:
        if self._handle is None:
            return
        self._model._runtime._lib.mmd_runtime_instance_free(self._handle)
        self._handle = None
        self._model._instances.discard(self)


class Clip(_OwnedHandle):
    """Owned opaque clip handle used by the minimal frame-evaluation smoke."""

    def __init__(self, runtime: RuntimeLibrary, handle: int) -> None:
        self._runtime = runtime
        self._handle: int | None = handle

    _closed_handle_message = "clip handle is closed"

    def frame_range(self) -> tuple[int, int]:
        first = ctypes.c_uint32()
        last = ctypes.c_uint32()
        if not self._runtime._lib.mmd_runtime_clip_frame_range(
            self._require_open(), ctypes.byref(first), ctypes.byref(last)
        ):
            raise self._runtime._failure("mmd_runtime_clip_frame_range")
        return first.value, last.value

    def close(self) -> None:
        if self._handle is None:
            return
        self._runtime._lib.mmd_runtime_clip_free(self._handle)
        self._handle = None


class PhysicsWorld(_OwnedHandle):
    """Owned opaque PMX-derived physics world for parameter inspection."""

    def __init__(self, runtime: RuntimeLibrary, handle: int) -> None:
        self._runtime = runtime
        self._handle: int | None = handle

    _closed_handle_message = "physics world handle is closed"

    def params_json(self) -> dict[str, object]:
        operation = "mmd_runtime_physics_params_get_json"
        decoded = self._runtime.decode_owned_json(
            self._runtime._lib.mmd_runtime_physics_params_get_json(
                self._require_open()
            ),
            operation,
        )
        if not isinstance(decoded, dict):
            raise NativeRuntimeError(f"{operation} did not return a JSON object")
        return decoded

    def set_params_json(self, params: Mapping[str, object]) -> None:
        if not isinstance(params, Mapping):
            raise TypeError("params must be a mapping")
        payload = json.dumps(
            dict(params),
            ensure_ascii=False,
            allow_nan=False,
            separators=(",", ":"),
            sort_keys=True,
        ).encode("utf-8")
        storage, length = self._runtime._input_bytes(payload)
        operation = "mmd_runtime_physics_params_set_json"
        status = self._runtime._lib.mmd_runtime_physics_params_set_json(
            self._require_open(), storage, length
        )
        self._runtime._require_ok(status, operation)

    def rigidbody_count(self) -> int:
        count = ctypes.c_size_t()
        status = self._runtime._lib.mmd_runtime_physics_world_rigidbody_count(
            self._require_open(), ctypes.byref(count)
        )
        self._runtime._require_ok(status, "mmd_runtime_physics_world_rigidbody_count")
        return count.value

    def rigidbody_bindings(self) -> list[PhysicsRigidbodyBinding]:
        count = self.rigidbody_count()
        if count == 0:
            return []
        output = (PhysicsRigidbodyBinding * count)()
        actual = ctypes.c_size_t()
        status = self._runtime._lib.mmd_runtime_physics_world_copy_rigidbody_bindings(
            self._require_open(), output, count, ctypes.byref(actual)
        )
        self._runtime._require_ok(
            status, "mmd_runtime_physics_world_copy_rigidbody_bindings"
        )
        return list(output[: actual.value])

    def rigidbody_states_f32(self) -> array:
        """Copy rigidbody position/quaternion state as contiguous f32 values."""

        count = self.rigidbody_count()
        if count == 0:
            return array("f")
        output = array("f", (0.0,)) * (count * 7)
        native_output = (ctypes.c_float * len(output)).from_buffer(output)
        status = self._runtime._lib.mmd_runtime_physics_world_copy_rigidbody_states(
            self._require_open(), native_output, len(output)
        )
        self._runtime._require_ok(
            status, "mmd_runtime_physics_world_copy_rigidbody_states"
        )
        return output

    def close(self) -> None:
        if self._handle is None:
            return
        self._runtime._lib.mmd_runtime_physics_world_free(self._handle)
        self._handle = None


class PmxGeometry(_OwnedHandle):
    """Owned parsed-PMX geometry handle."""

    def __init__(self, runtime: RuntimeLibrary, handle: int) -> None:
        self._runtime = runtime
        self._handle: int | None = handle

    _closed_handle_message = "PMX geometry handle is closed"

    def positions_bytes(self) -> bytes:
        return self._runtime.copy_owned_bytes(
            self._runtime._lib.mmd_runtime_pmx_geometry_positions_buffer(
                self._require_open()
            ),
            "mmd_runtime_pmx_geometry_positions_buffer",
        )

    def close(self) -> None:
        if self._handle is None:
            return
        self._runtime._lib.mmd_runtime_pmx_geometry_free(self._handle)
        self._handle = None


class IkChain(_OwnedHandle):
    """Owned IK-chain primitive with caller-owned solve output."""

    def __init__(
        self, runtime: RuntimeLibrary, handle: int, bone_count: int, link_count: int
    ) -> None:
        self._runtime = runtime
        self._handle: int | None = handle
        self._bone_count = bone_count
        self._link_count = link_count

    _closed_handle_message = "IK chain handle is closed"

    def solve(
        self,
        local_rotations_xyzw: Sequence[float],
        goal_position_xyz: Sequence[float],
        *,
        tolerance: float = 1.0e-3,
        max_iterations_cap: int = 0,
    ) -> tuple[list[float], IkSolveStats]:
        if len(local_rotations_xyzw) != self._bone_count * 4:
            raise ValueError("local rotations must contain four floats per bone")
        if len(goal_position_xyz) != 3:
            raise ValueError("goal position must contain three floats")
        rotations = (ctypes.c_float * len(local_rotations_xyzw))(
            *local_rotations_xyzw
        )
        goal = (ctypes.c_float * 3)(*goal_position_xyz)
        output = (ctypes.c_float * (self._link_count * 4))()
        stats = IkSolveStats()
        if not self._runtime._lib.mmd_runtime_ik_chain_solve(
            self._require_open(),
            None,
            None,
            rotations,
            goal,
            tolerance,
            max_iterations_cap,
            output,
            len(output),
            ctypes.byref(stats),
        ):
            raise self._runtime._failure("mmd_runtime_ik_chain_solve")
        return list(output), stats

    def close(self) -> None:
        if self._handle is None:
            return
        self._runtime._lib.mmd_runtime_ik_chain_free(self._handle)
        self._handle = None
