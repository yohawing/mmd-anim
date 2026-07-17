from __future__ import annotations

import ctypes
import json
import math
import os
import sys
import unittest
from array import array
from pathlib import Path


PYTHON_BINDING_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(PYTHON_BINDING_ROOT))

from mmd_anim._runtime import (  # noqa: E402
    EXPECTED_ABI_VERSION,
    FEATURE_PHYSICS_BULLET_NATIVE,
    LIBRARY_ENV,
    AbiVersionError,
    ByteBuffer,
    Clip,
    Instance,
    Model,
    NativeRuntimeError,
    NativeStatusError,
    PhysicsWorld,
    RigBone,
    RigIkLink,
    RuntimeLibrary,
    _copy_and_free_byte_buffer,
    _require_abi_version,
    local_release_library_path,
)
from mmd_anim._abi import ctypes_type  # noqa: E402


class PureBindingTests(unittest.TestCase):
    def test_native_physics_capability_bit(self) -> None:
        class FakeLibrary:
            def __init__(self, flags: int) -> None:
                self.flags = flags

            def mmd_runtime_feature_flags(self) -> int:
                return self.flags

        runtime = object.__new__(RuntimeLibrary)
        runtime._lib = FakeLibrary(0)
        self.assertEqual(runtime.feature_flags(), 0)
        self.assertFalse(runtime.supports_native_physics())

        runtime._lib = FakeLibrary(FEATURE_PHYSICS_BULLET_NATIVE)
        self.assertEqual(runtime.feature_flags(), FEATURE_PHYSICS_BULLET_NATIVE)
        self.assertTrue(runtime.supports_native_physics())

    def test_native_status_error_names_and_detail(self) -> None:
        self.assertIs(ctypes_type("mmd_runtime_status_t"), ctypes.c_int)
        self.assertIs(
            ctypes_type("mmd_runtime_physics_world_t**"),
            ctypes.POINTER(ctypes.c_void_p),
        )
        expected = {
            0: "OK",
            1: "INVALID_INPUT",
            2: "UNSUPPORTED",
            3: "BUFFER_TOO_SMALL",
            4: "ERROR",
        }
        for status, name in expected.items():
            with self.subTest(status=status):
                error = NativeStatusError("test_operation", status, "native detail")
                self.assertEqual(error.status, status)
                self.assertEqual(error.status_name, name)
                self.assertIn("native detail", str(error))

    def test_physics_world_create_validates_and_cleans_failed_output(self) -> None:
        class FakeLibrary:
            def __init__(self, status: int, write_handle: bool) -> None:
                self.status = status
                self.write_handle = write_handle
                self.create_calls = 0
                self.freed: list[int] = []

            def mmd_runtime_physics_world_create_from_pmx_bytes(
                self, data: object, length: int, out_world: object
            ) -> int:
                self.create_calls += 1
                if self.write_handle:
                    ctypes.cast(out_world, ctypes.POINTER(ctypes.c_void_p))[0] = 77
                return self.status

            def mmd_runtime_physics_world_free(self, world: int) -> None:
                self.freed.append(world)

            def mmd_runtime_last_error_message(self) -> bytes:
                return b"physics unavailable"

        empty_runtime = object.__new__(RuntimeLibrary)
        empty_runtime._lib = FakeLibrary(0, False)
        with self.assertRaisesRegex(ValueError, "must not be empty"):
            empty_runtime.create_physics_world_from_pmx_bytes(b"")
        self.assertEqual(empty_runtime._lib.create_calls, 0)

        failed_runtime = object.__new__(RuntimeLibrary)
        failed_runtime._lib = FakeLibrary(2, True)
        with self.assertRaises(NativeStatusError) as caught:
            failed_runtime.create_physics_world_from_pmx_bytes(b"pmx")
        self.assertEqual(caught.exception.status_name, "UNSUPPORTED")
        self.assertIn("physics unavailable", str(caught.exception))
        self.assertEqual(failed_runtime._lib.freed, [77])

        null_runtime = object.__new__(RuntimeLibrary)
        null_runtime._lib = FakeLibrary(0, False)
        with self.assertRaisesRegex(NativeRuntimeError, "NULL world"):
            null_runtime.create_physics_world_from_pmx_bytes(b"pmx")

    def test_physics_set_status_json_and_idempotent_close(self) -> None:
        class FakeLibrary:
            def __init__(self) -> None:
                self.payload = b""
                self.free_calls = 0

            def mmd_runtime_physics_params_set_json(
                self, world: int, data: object, length: int
            ) -> int:
                self.payload = bytes(data[:length])
                return 1

            def mmd_runtime_last_error_message(self) -> bytes:
                return "無効な設定".encode()

            def mmd_runtime_physics_world_free(self, world: int) -> None:
                self.free_calls += 1

        runtime = object.__new__(RuntimeLibrary)
        runtime._lib = FakeLibrary()
        world = PhysicsWorld(runtime, 9)

        with self.assertRaises(TypeError):
            world.set_params_json([("schema_version", 1)])  # type: ignore[arg-type]
        with self.assertRaises(NativeStatusError) as caught:
            world.set_params_json({"名前": "剛体", "schema_version": 1})
        self.assertEqual(caught.exception.status, 1)
        self.assertEqual(caught.exception.status_name, "INVALID_INPUT")
        self.assertIn("無効な設定", str(caught.exception))
        self.assertEqual(
            runtime._lib.payload,
            '{"schema_version":1,"名前":"剛体"}'.encode(),
        )

        world.close()
        world.close()
        self.assertEqual(runtime._lib.free_calls, 1)

    def test_f32_arrays_use_stdlib_storage_and_zero_length_policy(self) -> None:
        class FakeLibrary:
            def mmd_runtime_instance_morph_weight_len(self, instance: int) -> int:
                return 0

            def mmd_runtime_instance_world_matrix_f32_len(
                self, instance: int
            ) -> int:
                return 0

            def mmd_runtime_instance_skinning_matrix_f32_len(
                self, instance: int
            ) -> int:
                return 0

            def mmd_runtime_instance_copy_morph_weights(
                self, instance: int, output: object, length: int
            ) -> bool:
                raise AssertionError("zero-length morph output must not be constructed")

            def mmd_runtime_instance_copy_world_matrices(
                self, instance: int, output: object, length: int
            ) -> bool:
                raise AssertionError("zero-length world output must not be constructed")

            def mmd_runtime_instance_copy_skinning_matrices(
                self, instance: int, output: object, length: int
            ) -> bool:
                raise AssertionError(
                    "zero-length skinning output must not be constructed"
                )

            def mmd_runtime_last_error_message(self) -> None:
                return None

        runtime = object.__new__(RuntimeLibrary)
        runtime._lib = FakeLibrary()
        instance = Instance(Model(runtime, 1), 2)

        morphs = instance.morph_weights_f32()
        self.assertIsInstance(morphs, array)
        self.assertEqual(morphs.typecode, "f")
        self.assertEqual(morphs.itemsize, ctypes.sizeof(ctypes.c_float))
        self.assertEqual(len(morphs), 0)
        with self.assertRaisesRegex(
            NativeRuntimeError, "world_matrix_f32_len failed"
        ):
            instance.world_matrices_f32()
        with self.assertRaisesRegex(
            NativeRuntimeError, "skinning_matrix_f32_len failed"
        ):
            instance.skinning_matrices_f32()

    def test_empty_import_bytes_are_rejected_before_native_calls(self) -> None:
        class FakeLibrary:
            def __init__(self) -> None:
                self.model_import_calls = 0
                self.clip_import_calls = 0

            def mmd_runtime_model_create_from_pmx_bytes(
                self, data: object, length: int
            ) -> int:
                self.model_import_calls += 1
                return 1

            def mmd_runtime_clip_create_from_vmd_bytes_for_model(
                self, model: int, data: object, length: int
            ) -> int:
                self.clip_import_calls += 1
                return 1

        runtime = object.__new__(RuntimeLibrary)
        runtime._lib = FakeLibrary()
        model = Model(runtime, 1)

        with self.assertRaisesRegex(ValueError, "must not be empty"):
            runtime.create_model_from_pmx_bytes(b"")
        with self.assertRaisesRegex(ValueError, "must not be empty"):
            runtime.create_clip_from_vmd_bytes(model, b"")

        self.assertEqual(runtime._lib.model_import_calls, 0)
        self.assertEqual(runtime._lib.clip_import_calls, 0)

    def test_cross_runtime_model_import_is_rejected_before_native_call(self) -> None:
        class FakeLibrary:
            def __init__(self) -> None:
                self.clip_import_calls = 0

            def mmd_runtime_clip_create_from_vmd_bytes_for_model(
                self, model: int, data: object, length: int
            ) -> int:
                self.clip_import_calls += 1
                return 1

        runtime = object.__new__(RuntimeLibrary)
        runtime._lib = FakeLibrary()
        foreign_runtime = object.__new__(RuntimeLibrary)
        foreign_runtime._lib = FakeLibrary()

        with self.assertRaisesRegex(ValueError, "same RuntimeLibrary"):
            runtime.create_clip_from_vmd_bytes(Model(foreign_runtime, 1), b"vmd")

        self.assertEqual(runtime._lib.clip_import_calls, 0)

    def test_const_char_pointer_and_borrowed_last_error_copy(self) -> None:
        self.assertIs(ctypes_type("const char*"), ctypes.c_char_p)

        class FakeLibrary:
            def __init__(self) -> None:
                self.free_calls = 0
                self.storage = ctypes.create_string_buffer(b"integer pointer failure")
                self.values: list[bytes | int] = [
                    "native failure".encode("utf-8"),
                    ctypes.addressof(self.storage),
                ]

            def mmd_runtime_last_error_message(self) -> bytes | int:
                return self.values.pop(0)

            def mmd_runtime_byte_buffer_free(self, value: ByteBuffer) -> None:
                self.free_calls += 1

        native = FakeLibrary()
        runtime = object.__new__(RuntimeLibrary)
        runtime._lib = native

        self.assertEqual(runtime._last_error(), "native failure")
        self.assertEqual(runtime._last_error(), "integer pointer failure")
        self.assertEqual(native.free_calls, 0)

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
                    matrices = instance.world_matrices_f32()
                clip.close()  # Idempotent after context-manager cleanup.

        self.assertEqual(len(matrices), 16)
        self.assertAlmostEqual(matrices[12], 1.0)
        self.assertAlmostEqual(matrices[13], 2.0)
        self.assertAlmostEqual(matrices[14], 3.0)


class NativeRepresentativeSmoke(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        explicit = os.environ.get(LIBRARY_ENV)
        fallback = local_release_library_path()
        if explicit is None and not fallback.is_file():
            raise unittest.SkipTest(
                f"native library absent; build release or set {LIBRARY_ENV}"
            )
        cls.runtime = RuntimeLibrary()
        fixtures = (
            Path(__file__).resolve().parents[3]
            / "crates"
            / "mmd-anim-format"
            / "fixtures"
        )
        cls.pmx = (fixtures / "pmx" / "ik_multi_axis_limit.pmx").read_bytes()
        cls.vmd = (fixtures / "vmd" / "ik_multi_bone_nondefault.vmd").read_bytes()

    def test_repository_fixture_parser_json_semantics(self) -> None:
        pmx = self.runtime.parse_pmx_non_geometry_json(self.pmx)
        self.assertIsInstance(pmx, dict)
        self.assertEqual(pmx["metadata"]["format"], "pmx")
        self.assertEqual(pmx["metadata"]["counts"]["bones"], 3)
        self.assertEqual(pmx["skeleton"]["bones"][2]["ik"]["targetIndex"], 1)

        vmd = self.runtime.parse_vmd_json(self.vmd)
        self.assertIsInstance(vmd, dict)
        self.assertEqual(vmd["kind"], "vmd")
        self.assertEqual(vmd["metadata"]["counts"]["bones"], 5)
        self.assertEqual(vmd["metadata"]["maxFrame"], 30)
        self.assertEqual(vmd["boneFrames"][3]["frame"], 15)

    def test_physics_parameter_json_roundtrip_is_fail_atomic(self) -> None:
        if not self.runtime.supports_native_physics():
            self.skipTest(
                "release DLL does not advertise native Bullet physics capability"
            )
        world = self.runtime.create_physics_world_from_pmx_bytes(self.pmx)
        self.addCleanup(world.close)

        before = world.params_json()
        self.assertEqual(
            before,
            {"schema_version": 1, "rigid_bodies": {}, "joints": {}},
        )
        world.set_params_json(before)
        self.assertEqual(world.params_json(), before)

        invalid = dict(before)
        invalid["schema_version"] = 2
        with self.assertRaises(NativeStatusError) as caught:
            world.set_params_json(invalid)
        self.assertEqual(caught.exception.status, 1)
        self.assertEqual(caught.exception.status_name, "INVALID_INPUT")
        self.assertEqual(world.params_json(), before)

        world.close()
        world.close()

    def test_repository_fixture_import_evaluate_and_idempotent_free(self) -> None:
        model = self.runtime.create_model_from_pmx_bytes(self.pmx)
        self.addCleanup(model.close)
        clip = self.runtime.create_clip_from_vmd_bytes(model, self.vmd)
        self.addCleanup(clip.close)
        instance = model.create_instance_for_model()
        self.addCleanup(instance.close)

        self.assertEqual(model.bone_count(), 3)
        self.assertEqual(model.morph_count(), 0)
        self.assertEqual(clip.frame_range(), (0, 30))

        instance.evaluate_clip_frame(clip, 15.0)
        world = instance.world_matrices_f32()
        skinning = instance.skinning_matrices_f32()
        morphs = instance.morph_weights_f32()
        self.assertEqual(len(world), model.bone_count() * 16)
        self.assertEqual(len(skinning), model.bone_count() * 16)
        self.assertEqual(len(morphs), model.morph_count())
        for output in (world, skinning, morphs):
            self.assertIsInstance(output, array)
            self.assertEqual(output.typecode, "f")
        self.assertTrue(all(math.isfinite(value) for value in world))
        self.assertTrue(all(math.isfinite(value) for value in skinning))
        self.assertTrue(all(math.isfinite(value) for value in morphs))

        model.close()  # Cascades the still-live instance.
        model.close()
        instance.close()
        clip.close()
        clip.close()

    def test_pmx_geometry_positions_buffer_and_idempotent_free(self) -> None:
        geometry = self.runtime.create_pmx_geometry(self.pmx)
        payload = geometry.positions_bytes()
        geometry.close()
        geometry.close()

        self.assertGreater(len(payload), 0)
        self.assertEqual(len(payload) % ctypes.sizeof(ctypes.c_float), 0)
        self.assertEqual(len(payload) // ctypes.sizeof(ctypes.c_float), 9)

    def test_ik_chain_create_solve_and_idempotent_free(self) -> None:
        zero3 = (ctypes.c_float * 3)(0.0, 0.0, 0.0)
        bones = [
            RigBone(-1, zero3, 0, zero3),
            RigBone(0, (ctypes.c_float * 3)(1.0, 0.0, 0.0), 0, zero3),
        ]
        links = [RigIkLink(0, False, zero3, zero3)]
        chain = self.runtime.create_ik_chain(bones, 1, links, 4, 0.0)
        output, stats = chain.solve(
            [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
            [0.0, 1.0, 0.0],
        )
        chain.close()
        chain.close()

        self.assertEqual(len(output), 4)
        self.assertLessEqual(stats.final_distance, 1.0e-3)
        self.assertEqual(stats.break_reason, 0)
        self.assertAlmostEqual(output[2], 2.0**-0.5, places=4)
        self.assertAlmostEqual(output[3], 2.0**-0.5, places=4)


if __name__ == "__main__":
    unittest.main()
