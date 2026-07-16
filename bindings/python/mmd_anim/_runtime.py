"""Minimal, internal ctypes binding for the mmd-anim runtime C ABI v2."""

from __future__ import annotations

import ctypes
import json
import os
import sys
from pathlib import Path
from typing import Callable, Sequence


EXPECTED_ABI_VERSION = 2
LIBRARY_ENV = "MMD_RUNTIME_LIBRARY"


class NativeRuntimeError(RuntimeError):
    """Raised when loading or calling the native runtime fails."""


class AbiVersionError(NativeRuntimeError):
    """Raised when the loaded native library does not implement ABI v2."""


class ByteBuffer(ctypes.Structure):
    """Value representation of ``mmd_runtime_ffi_byte_buffer_t``."""

    _fields_ = [
        ("data", ctypes.POINTER(ctypes.c_uint8)),
        ("len", ctypes.c_size_t),
    ]


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

        lib.mmd_runtime_abi_version.argtypes = []
        lib.mmd_runtime_abi_version.restype = ctypes.c_uint32
        _require_abi_version(lib.mmd_runtime_abi_version())

        lib.mmd_runtime_last_error_message.argtypes = []
        lib.mmd_runtime_last_error_message.restype = ctypes.c_void_p

        lib.mmd_runtime_byte_buffer_free.argtypes = [ByteBuffer]
        lib.mmd_runtime_byte_buffer_free.restype = None

        lib.mmd_runtime_model_create.argtypes = [
            ctypes.POINTER(ctypes.c_int32),
            ctypes.POINTER(ctypes.c_float),
            ctypes.c_size_t,
        ]
        lib.mmd_runtime_model_create.restype = ctypes.c_void_p
        lib.mmd_runtime_model_free.argtypes = [ctypes.c_void_p]
        lib.mmd_runtime_model_free.restype = None

        lib.mmd_runtime_instance_create.argtypes = [ctypes.c_void_p, ctypes.c_size_t]
        lib.mmd_runtime_instance_create.restype = ctypes.c_void_p
        lib.mmd_runtime_instance_free.argtypes = [ctypes.c_void_p]
        lib.mmd_runtime_instance_free.restype = None
        lib.mmd_runtime_instance_evaluate_rest_pose.argtypes = [ctypes.c_void_p]
        lib.mmd_runtime_instance_evaluate_rest_pose.restype = ctypes.c_bool
        lib.mmd_runtime_instance_world_matrix_f32_len.argtypes = [ctypes.c_void_p]
        lib.mmd_runtime_instance_world_matrix_f32_len.restype = ctypes.c_size_t
        lib.mmd_runtime_instance_copy_world_matrices.argtypes = [
            ctypes.c_void_p,
            ctypes.POINTER(ctypes.c_float),
            ctypes.c_size_t,
        ]
        lib.mmd_runtime_instance_copy_world_matrices.restype = ctypes.c_bool

        lib.mmd_runtime_clip_create.argtypes = [
            ctypes.c_void_p,
            ctypes.c_size_t,
            ctypes.c_void_p,
            ctypes.c_size_t,
            ctypes.c_void_p,
            ctypes.c_size_t,
            ctypes.c_void_p,
            ctypes.c_size_t,
            ctypes.c_void_p,
            ctypes.c_size_t,
            ctypes.c_void_p,
            ctypes.c_size_t,
        ]
        lib.mmd_runtime_clip_create.restype = ctypes.c_void_p
        lib.mmd_runtime_clip_free.argtypes = [ctypes.c_void_p]
        lib.mmd_runtime_clip_free.restype = None
        lib.mmd_runtime_instance_evaluate_clip_frame.argtypes = [
            ctypes.c_void_p,
            ctypes.c_void_p,
            ctypes.c_float,
        ]
        lib.mmd_runtime_instance_evaluate_clip_frame.restype = ctypes.c_bool

    def _last_error(self) -> str | None:
        pointer = self._lib.mmd_runtime_last_error_message()
        if not pointer:
            return None
        # Copy immediately: this thread-local pointer expires on the next FFI call.
        return ctypes.string_at(pointer).decode("utf-8", errors="replace")

    def _failure(self, operation: str) -> NativeRuntimeError:
        detail = self._last_error()
        return NativeRuntimeError(f"{operation} failed" + (f": {detail}" if detail else ""))

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


class Model:
    """Owned opaque model handle."""

    def __init__(self, runtime: RuntimeLibrary, handle: int) -> None:
        self._runtime = runtime
        self._handle: int | None = handle
        self._instances: set[Instance] = set()

    def __enter__(self) -> Model:
        self._require_open()
        return self

    def __exit__(self, exc_type: object, exc: object, traceback: object) -> None:
        self.close()

    def _require_open(self) -> int:
        if self._handle is None:
            raise NativeRuntimeError("model handle is closed")
        return self._handle

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

    def close(self) -> None:
        if self._handle is None:
            return
        for instance in tuple(self._instances):
            instance.close()
        self._runtime._lib.mmd_runtime_model_free(self._handle)
        self._handle = None


class Instance:
    """Owned opaque runtime-instance handle."""

    def __init__(self, model: Model, handle: int) -> None:
        self._model = model
        self._handle: int | None = handle

    def __enter__(self) -> Instance:
        self._require_open()
        return self

    def __exit__(self, exc_type: object, exc: object, traceback: object) -> None:
        self.close()

    def _require_open(self) -> int:
        if self._handle is None:
            raise NativeRuntimeError("instance handle is closed")
        return self._handle

    def evaluate_rest_pose(self) -> None:
        if not self._model._runtime._lib.mmd_runtime_instance_evaluate_rest_pose(
            self._require_open()
        ):
            raise self._model._runtime._failure(
                "mmd_runtime_instance_evaluate_rest_pose"
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

    def world_matrices(self) -> list[float]:
        handle = self._require_open()
        length = self._model._runtime._lib.mmd_runtime_instance_world_matrix_f32_len(
            handle
        )
        if length == 0:
            raise self._model._runtime._failure(
                "mmd_runtime_instance_world_matrix_f32_len"
            )
        output = (ctypes.c_float * length)()
        if not self._model._runtime._lib.mmd_runtime_instance_copy_world_matrices(
            handle, output, length
        ):
            raise self._model._runtime._failure(
                "mmd_runtime_instance_copy_world_matrices"
            )
        return list(output)

    def close(self) -> None:
        if self._handle is None:
            return
        self._model._runtime._lib.mmd_runtime_instance_free(self._handle)
        self._handle = None
        self._model._instances.discard(self)


class Clip:
    """Owned opaque clip handle used by the minimal frame-evaluation smoke."""

    def __init__(self, runtime: RuntimeLibrary, handle: int) -> None:
        self._runtime = runtime
        self._handle: int | None = handle

    def __enter__(self) -> Clip:
        self._require_open()
        return self

    def __exit__(self, exc_type: object, exc: object, traceback: object) -> None:
        self.close()

    def _require_open(self) -> int:
        if self._handle is None:
            raise NativeRuntimeError("clip handle is closed")
        return self._handle

    def close(self) -> None:
        if self._handle is None:
            return
        self._runtime._lib.mmd_runtime_clip_free(self._handle)
        self._handle = None
