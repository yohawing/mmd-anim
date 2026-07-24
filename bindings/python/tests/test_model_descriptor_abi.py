from __future__ import annotations

import ctypes
import os
import sys
import threading
import unittest
from pathlib import Path


PYTHON_BINDING_ROOT = Path(__file__).resolve().parents[1]
REPOSITORY_ROOT = PYTHON_BINDING_ROOT.parents[1]
sys.path.insert(0, str(PYTHON_BINDING_ROOT))

from mmd_anim._abi import (  # noqa: E402
    MODEL_DESCRIPTOR_MANIFEST,
    ModelBoneDescriptor,
    ModelDescriptor,
    bind_functions,
)
from mmd_anim._runtime import (  # noqa: E402
    EXPECTED_ABI_VERSION,
    LIBRARY_ENV,
    local_release_library_path,
)


FEATURE_MODEL_DESCRIPTOR = int(MODEL_DESCRIPTOR_MANIFEST["feature"]["value"])
DESCRIPTOR_VERSION = int(MODEL_DESCRIPTOR_MANIFEST["descriptor_version"])


def _library_path() -> Path:
    configured = os.environ.get(LIBRARY_ENV)
    return Path(configured) if configured else local_release_library_path()


def _descriptor(bones: ctypes.Array[ModelBoneDescriptor]) -> ModelDescriptor:
    return ModelDescriptor(
        struct_size=ctypes.sizeof(ModelDescriptor),
        descriptor_version=DESCRIPTOR_VERSION,
        flags=0,
        reserved=0,
        bones=bones,
        bone_count=len(bones),
        ik_solvers=None,
        ik_solver_count=0,
        ik_links=None,
        ik_link_count=0,
        append_transforms=None,
        append_transform_count=0,
        morph_count=0,
        bone_morph_offsets=None,
        bone_morph_offset_count=0,
        group_morph_offsets=None,
        group_morph_offset_count=0,
    )


@unittest.skipUnless(
    _library_path().is_file(),
    "release cdylib not found; build mmd-anim-ffi --release to run native smoke",
)
class NativeModelDescriptorAbiTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.library = ctypes.CDLL(str(_library_path().resolve()))
        bind_functions(cls.library)
        if cls.library.mmd_runtime_abi_version() != EXPECTED_ABI_VERSION:
            raise RuntimeError("release cdylib is not ABI v3")
        if not cls.library.mmd_runtime_feature_flags() & FEATURE_MODEL_DESCRIPTOR:
            raise RuntimeError("release cdylib lacks model-descriptor feature")
        if not hasattr(cls.library, "mmd_runtime_model_create_from_descriptor"):
            raise RuntimeError("release cdylib lacks descriptor constructor symbol")

    def _last_error(self) -> str:
        value = self.library.mmd_runtime_last_error_message()
        return value.decode("utf-8", errors="replace") if value else ""

    def _make_bone(self, parent_index: int = -1) -> ctypes.Array[ModelBoneDescriptor]:
        return (ModelBoneDescriptor * 1)(
            ModelBoneDescriptor(
                parent_index=parent_index,
                rest_position_xyz=(0.0, 0.0, 0.0),
                transform_order=0,
                flags=0,
                fixed_axis_xyz=(0.0, 0.0, 0.0),
                local_axis_x_xyz=(0.0, 0.0, 0.0),
                local_axis_z_xyz=(0.0, 0.0, 0.0),
            )
        )

    def _evaluate_descriptor(self, descriptor: ModelDescriptor) -> tuple[bytes, bytes]:
        model = self.library.mmd_runtime_model_create_from_descriptor(
            ctypes.byref(descriptor)
        )
        self.assertTrue(model)
        instance = self.library.mmd_runtime_instance_create_for_model(model)
        self.assertTrue(instance)
        try:
            self.assertTrue(self.library.mmd_runtime_instance_evaluate_rest_pose(instance))
            world_len = self.library.mmd_runtime_instance_world_matrix_f32_len(instance)
            skinning_len = self.library.mmd_runtime_instance_skinning_matrix_f32_len(instance)
            world = (ctypes.c_float * world_len)()
            skinning = (ctypes.c_float * skinning_len)()
            self.assertTrue(
                self.library.mmd_runtime_instance_copy_world_matrices(
                    instance, world, world_len
                )
            )
            self.assertTrue(
                self.library.mmd_runtime_instance_copy_skinning_matrices(
                    instance, skinning, skinning_len
                )
            )
            return ctypes.string_at(world, ctypes.sizeof(world)), ctypes.string_at(
                skinning, ctypes.sizeof(skinning)
            )
        finally:
            self.library.mmd_runtime_instance_free(instance)
            self.library.mmd_runtime_model_free(model)

    def test_descriptor_create_rest_matrix_copy_is_bitwise_deterministic(self) -> None:
        # Reuse one immutable descriptor allocation for two independent model
        # handles; the native constructor must copy borrowed records before it
        # returns and produce the same bytes on each fresh creation.
        bones = self._make_bone()
        descriptor = _descriptor(bones)
        first = self._evaluate_descriptor(descriptor)
        second = self._evaluate_descriptor(descriptor)
        self.assertEqual(first, second)

    def test_invalid_descriptor_is_no_crash_and_reports_indexed_last_error(self) -> None:
        invalid_bones = self._make_bone(parent_index=3)
        descriptor = _descriptor(invalid_bones)
        self.assertFalse(
            self.library.mmd_runtime_model_create_from_descriptor(
                ctypes.byref(descriptor)
            )
        )
        self.assertIn("bones[0]", self._last_error())

        descriptor.struct_size = 0
        self.assertFalse(
            self.library.mmd_runtime_model_create_from_descriptor(
                ctypes.byref(descriptor)
            )
        )
        self.assertIn("descriptor.struct_size", self._last_error())

    def test_indexed_last_error_is_isolated_between_native_threads(self) -> None:
        self.library.mmd_runtime_abi_version()
        self.assertEqual(self._last_error(), "")
        barrier = threading.Barrier(3)
        observed: dict[str, list[str]] = {}
        handles: dict[str, object] = {}

        def fail_with_parent(label: str, invalid_index: int) -> None:
            bones = (ModelBoneDescriptor * 2)(
                self._make_bone()[0],
                self._make_bone(parent_index=invalid_index)[0],
            )
            descriptor = _descriptor(bones)
            handles[label] = self.library.mmd_runtime_model_create_from_descriptor(
                ctypes.byref(descriptor)
            )
            barrier.wait(timeout=5)
            observed[label] = [self._last_error() for _ in range(32)]

        first = threading.Thread(target=fail_with_parent, args=("first", 7))
        second = threading.Thread(target=fail_with_parent, args=("second", 11))
        first.start()
        second.start()
        barrier.wait(timeout=5)
        first.join(timeout=5)
        second.join(timeout=5)

        self.assertFalse(first.is_alive())
        self.assertFalse(second.is_alive())
        self.assertFalse(handles["first"])
        self.assertFalse(handles["second"])
        self.assertEqual(self._last_error(), "")
        self.assertTrue(all("bones[1].parent" in value for value in observed["first"]))
        self.assertTrue(all("bones[1].parent" in value for value in observed["second"]))
        self.assertTrue(all("7" in value for value in observed["first"]))
        self.assertTrue(all("11" in value for value in observed["second"]))


if __name__ == "__main__":
    unittest.main()
