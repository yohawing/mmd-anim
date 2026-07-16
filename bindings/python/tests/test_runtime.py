from __future__ import annotations

import ctypes
import json
import os
import sys
import unittest
from pathlib import Path


PYTHON_BINDING_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(PYTHON_BINDING_ROOT))

from mmd_anim._runtime import (  # noqa: E402
    EXPECTED_ABI_VERSION,
    LIBRARY_ENV,
    AbiVersionError,
    ByteBuffer,
    Clip,
    Instance,
    Model,
    NativeRuntimeError,
    RuntimeLibrary,
    _copy_and_free_byte_buffer,
    _require_abi_version,
    local_release_library_path,
)


class PureBindingTests(unittest.TestCase):
    def test_abi_version_guard_accepts_only_v2(self) -> None:
        _require_abi_version(EXPECTED_ABI_VERSION)
        with self.assertRaisesRegex(AbiVersionError, "expected 2, got 1"):
            _require_abi_version(1)

    def test_owned_buffer_is_copied_and_freed_once(self) -> None:
        storage = (ctypes.c_uint8 * 3)(1, 2, 3)
        buffer = ByteBuffer(ctypes.cast(storage, ctypes.POINTER(ctypes.c_uint8)), 3)
        freed: list[int] = []

        copied = _copy_and_free_byte_buffer(buffer, lambda value: freed.append(value.len))

        self.assertEqual(copied, b"\x01\x02\x03")
        self.assertEqual(freed, [3])

    def test_owned_json_is_freed_before_decode_errors_escape(self) -> None:
        class FakeLibrary:
            def __init__(self) -> None:
                self.freed: list[int] = []

            def mmd_runtime_byte_buffer_free(self, value: ByteBuffer) -> None:
                self.freed.append(value.len)

        cases = ((b"\xff", UnicodeDecodeError), (b"not json", json.JSONDecodeError))
        for payload, error_type in cases:
            with self.subTest(payload=payload):
                native = FakeLibrary()
                runtime = object.__new__(RuntimeLibrary)
                runtime._lib = native
                storage = (ctypes.c_uint8 * len(payload))(*payload)
                buffer = ByteBuffer(
                    ctypes.cast(storage, ctypes.POINTER(ctypes.c_uint8)), len(payload)
                )

                with self.assertRaises(error_type):
                    runtime.decode_owned_json(buffer, "test_json")

                self.assertEqual(native.freed, [len(payload)])

    def test_cross_runtime_clip_is_rejected_before_native_call(self) -> None:
        class FakeLibrary:
            def __init__(self) -> None:
                self.evaluate_calls = 0

            def mmd_runtime_instance_evaluate_clip_frame(
                self, instance: int, clip: int, frame: float
            ) -> bool:
                self.evaluate_calls += 1
                return True

        instance_runtime = object.__new__(RuntimeLibrary)
        instance_runtime._lib = FakeLibrary()
        clip_runtime = object.__new__(RuntimeLibrary)
        clip_runtime._lib = FakeLibrary()
        instance = Instance(Model(instance_runtime, 1), 2)
        foreign_clip = Clip(clip_runtime, 3)

        with self.assertRaisesRegex(ValueError, "same RuntimeLibrary"):
            instance.evaluate_clip_frame(foreign_clip, 12.5)

        self.assertEqual(instance_runtime._lib.evaluate_calls, 0)

    def test_local_release_fallback_names_platform_library(self) -> None:
        path = local_release_library_path()
        self.assertEqual(path.parent.name, "release")
        self.assertIn(path.suffix, {".dll", ".so", ".dylib"})


class NativeLifecycleSmoke(unittest.TestCase):
    def test_one_bone_clip_frame_matrix_readback_and_free(self) -> None:
        explicit = os.environ.get(LIBRARY_ENV)
        fallback = local_release_library_path()
        if explicit is None and not fallback.is_file():
            self.skipTest(
                f"native library absent; build release or set {LIBRARY_ENV}"
            )

        runtime = RuntimeLibrary()
        with runtime.create_model([-1], [1.0, 2.0, 3.0]) as model:
            with model.create_instance() as instance:
                clip = runtime.create_empty_clip()
                with clip:
                    instance.evaluate_clip_frame(clip, 12.5)
                    matrices = instance.world_matrices()
                clip.close()  # Idempotent after context-manager cleanup.

        self.assertEqual(len(matrices), 16)
        self.assertAlmostEqual(matrices[12], 1.0)
        self.assertAlmostEqual(matrices[13], 2.0)
        self.assertAlmostEqual(matrices[14], 3.0)


if __name__ == "__main__":
    unittest.main()
