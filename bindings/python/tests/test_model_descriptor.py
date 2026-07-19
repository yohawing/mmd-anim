from __future__ import annotations

import ctypes
import gc
import math
import os
import sys
import weakref
import unittest
from pathlib import Path


PYTHON_BINDING_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(PYTHON_BINDING_ROOT))

from mmd_anim._abi import (  # noqa: E402
    MODEL_DESCRIPTOR_MANIFEST,
    ModelDescriptor,
)
from mmd_anim._model_descriptor import (  # noqa: E402
    APPEND_LOCAL,
    APPEND_ROTATION,
    APPEND_TRANSLATION,
    FEATURE_MODEL_DESCRIPTOR,
    IK_LINK_ANGLE_LIMIT,
    MODEL_BONE_FIXED_AXIS,
    MODEL_BONE_LOCAL_AXIS,
    MODEL_BONE_TRANSFORM_AFTER_PHYSICS,
    AppendTransform,
    Bone,
    BoneMorphOffset,
    GroupMorphOffset,
    IkLink,
    IkSolver,
    ModelDefinition,
    marshal_model_definition,
)
from mmd_anim._runtime import NativeRuntimeError, RuntimeLibrary  # noqa: E402


class ModelDescriptorPureTests(unittest.TestCase):
    def test_empty_definition_uses_null_and_zero_for_all_six_arrays(self) -> None:
        carrier = marshal_model_definition(ModelDefinition(()))
        descriptor = carrier.descriptor
        for pointer_name, count_name in (
            ("bones", "bone_count"),
            ("ik_solvers", "ik_solver_count"),
            ("ik_links", "ik_link_count"),
            ("append_transforms", "append_transform_count"),
            ("bone_morph_offsets", "bone_morph_offset_count"),
            ("group_morph_offsets", "group_morph_offset_count"),
        ):
            with self.subTest(pointer_name=pointer_name):
                self.assertFalse(getattr(descriptor, pointer_name))
                self.assertEqual(getattr(descriptor, count_name), 0)

    def test_frozen_records_and_all_record_flags_and_counts(self) -> None:
        definition = ModelDefinition(
            bones=(
                Bone(None, (0.0, 0.0, 0.0)),
                Bone(
                    0,
                    (0.0, 2.0, 0.0),
                    transform_order=3,
                    transform_after_physics=True,
                    fixed_axis_xyz=(1.0, 0.0, 0.0),
                    local_axis_x_xyz=(1.0, 0.0, 0.0),
                    local_axis_z_xyz=(0.0, 0.0, 1.0),
                ),
            ),
            ik_solvers=(
                IkSolver(
                    1,
                    0,
                    (IkLink(0, (-1.0, -2.0, -3.0), (1.0, 2.0, 3.0)),),
                ),
            ),
            append_transforms=(AppendTransform(1, 0, 0.5, True, True, True),),
            morph_count=2,
            bone_morph_offsets=(BoneMorphOffset(0, 1, (1.0, 2.0, 3.0), (0, 0, 0, 1)),),
            group_morph_offsets=(GroupMorphOffset(0, 1, 0.25),),
        )
        carrier = marshal_model_definition(definition)
        self.assertEqual(carrier.descriptor.bone_count, 2)
        self.assertEqual(carrier.descriptor.ik_solver_count, 1)
        self.assertEqual(carrier.descriptor.ik_link_count, 1)
        self.assertEqual(carrier.descriptor.append_transform_count, 1)
        self.assertEqual(carrier.descriptor.morph_count, 2)
        self.assertEqual(carrier.descriptor.bone_morph_offset_count, 1)
        self.assertEqual(carrier.descriptor.group_morph_offset_count, 1)
        for pointer_name, count_name in (
            ("bones", "bone_count"),
            ("ik_solvers", "ik_solver_count"),
            ("ik_links", "ik_link_count"),
            ("append_transforms", "append_transform_count"),
            ("bone_morph_offsets", "bone_morph_offset_count"),
            ("group_morph_offsets", "group_morph_offset_count"),
        ):
            with self.subTest(pointer_name=pointer_name):
                self.assertTrue(getattr(carrier.descriptor, pointer_name))
                self.assertGreater(getattr(carrier.descriptor, count_name), 0)
        self.assertEqual(
            carrier.bones[1].flags,
            MODEL_BONE_TRANSFORM_AFTER_PHYSICS
            | MODEL_BONE_FIXED_AXIS
            | MODEL_BONE_LOCAL_AXIS,
        )
        self.assertEqual(carrier.ik_links[0].flags, IK_LINK_ANGLE_LIMIT)
        self.assertEqual(
            carrier.append_transforms[0].flags,
            APPEND_ROTATION | APPEND_TRANSLATION | APPEND_LOCAL,
        )
        self.assertEqual(
            carrier.descriptor.descriptor_version,
            int(MODEL_DESCRIPTOR_MANIFEST["descriptor_version"]),
        )
        self.assertEqual(carrier.descriptor.flags, 0)
        self.assertEqual(carrier.descriptor.reserved, 0)

    def test_ik_links_are_flattened_with_solver_offsets(self) -> None:
        carrier = marshal_model_definition(
            ModelDefinition(
                bones=(Bone(None, (0, 0, 0)),),
                ik_solvers=(
                    IkSolver(0, 0, (IkLink(0), IkLink(0))),
                    IkSolver(0, 0, (IkLink(0),)),
                ),
            )
        )
        self.assertEqual(carrier.descriptor.ik_link_count, 3)
        self.assertEqual(carrier.ik_solvers[0].link_offset, 0)
        self.assertEqual(carrier.ik_solvers[0].link_count, 2)
        self.assertEqual(carrier.ik_solvers[1].link_offset, 2)
        self.assertEqual(carrier.ik_solvers[1].link_count, 1)

    def test_bone_position_is_the_contiguous_native_index(self) -> None:
        definition = ModelDefinition((Bone(None, (0, 0, 0)), Bone(0, (1, 0, 0))))
        self.assertFalse(hasattr(definition.bones[0], "index"))
        carrier = marshal_model_definition(definition)
        self.assertEqual([carrier.bones[i].parent_index for i in range(2)], [-1, 0])

    def test_marshal_safety_rejects_bad_shapes_and_unpaired_axes(self) -> None:
        with self.assertRaises(ValueError):
            Bone(None, (0, 0))
        with self.assertRaises(ValueError):
            Bone(None, (0, 0, 0), fixed_axis_xyz=(1, 0))
        with self.assertRaises(ValueError):
            IkLink(0, angle_limit_min_xyz=(0, 0), angle_limit_max_xyz=(1, 1, 1))
        with self.assertRaises(ValueError):
            BoneMorphOffset(0, 0, (0, 0), (0, 0, 0, 1))
        with self.assertRaises(ValueError):
            BoneMorphOffset(0, 0, (0, 0, 0), (0, 0, 0))
        with self.assertRaises(ValueError):
            marshal_model_definition(
                ModelDefinition((Bone(None, (0, 0, 0), local_axis_x_xyz=(1, 0, 0)),))
            )
        with self.assertRaises(ValueError):
            marshal_model_definition(
                ModelDefinition((Bone(None, (0, 0, 0)),), morph_count=-1)
            )
        with self.assertRaises(TypeError):
            marshal_model_definition(
                ModelDefinition((Bone(None, (0, 0, 0), transform_after_physics=1),))
            )
        with self.assertRaises(ValueError):
            marshal_model_definition(
                ModelDefinition(
                    (Bone(None, (0, 0, 0)),),
                    ik_solvers=(IkSolver(0, 0, (IkLink(0, (0, 0, 0), None),)),),
                )
            )


class _FakeLibrary:
    def __init__(self, *, feature_flags: int, has_constructor: bool = True) -> None:
        self.feature_flags_value = feature_flags
        self.constructor_calls = 0
        self.observed: tuple[int, int, int] | None = None
        self.observed_payload: dict[str, object] | None = None
        self.model_free_calls = 0
        self.error = b"descriptor.bones[3].parent: invalid"
        if has_constructor:
            self.mmd_runtime_model_create_from_descriptor = self._create

    def mmd_runtime_feature_flags(self) -> int:
        return self.feature_flags_value

    def mmd_runtime_last_error_message(self) -> bytes:
        return self.error

    def _create(self, pointer: object) -> int:
        self.constructor_calls += 1
        descriptor = ctypes.cast(pointer, ctypes.POINTER(ModelDescriptor)).contents
        self.observed = (
            descriptor.bone_count,
            descriptor.ik_solver_count,
            descriptor.ik_link_count,
        )
        self.observed_payload = {
            "bones": (
                descriptor.bones[0].parent_index,
                descriptor.bones[1].rest_position_xyz[1],
            ),
            "ik_solvers": (
                descriptor.ik_solvers[0].ik_bone_index,
                descriptor.ik_solvers[0].link_offset,
                descriptor.ik_solvers[0].link_count,
            ),
            "ik_links": (
                descriptor.ik_links[0].bone_index,
                descriptor.ik_links[0].flags,
            ),
            "append_transforms": (
                descriptor.append_transforms[0].target_bone_index,
                descriptor.append_transforms[0].source_bone_index,
            ),
            "bone_morph_offsets": (
                descriptor.bone_morph_offsets[0].morph_index,
                descriptor.bone_morph_offsets[0].target_bone_index,
            ),
            "group_morph_offsets": (
                descriptor.group_morph_offsets[0].morph_index,
                descriptor.group_morph_offsets[0].child_morph_index,
            ),
        }
        # A fake native constructor observes the borrowed carrier while the
        # call is active, then returns a stand-in opaque handle.
        return 1234

    def mmd_runtime_model_free(self, handle: int) -> None:
        self.model_free_calls += 1


class ModelDescriptorRuntimeTests(unittest.TestCase):
    def _runtime(self, fake: _FakeLibrary) -> RuntimeLibrary:
        runtime = object.__new__(RuntimeLibrary)
        runtime._lib = fake
        return runtime

    def test_feature_and_symbol_guard_prevents_native_call(self) -> None:
        fake = _FakeLibrary(feature_flags=0)
        runtime = self._runtime(fake)
        self.assertFalse(runtime.supports_model_descriptor())
        with self.assertRaises(NativeRuntimeError):
            runtime.create_model_from_descriptor(
                ModelDefinition((Bone(None, (0, 0, 0)),))
            )
        self.assertEqual(fake.constructor_calls, 0)

        fake = _FakeLibrary(
            feature_flags=FEATURE_MODEL_DESCRIPTOR, has_constructor=False
        )
        runtime = self._runtime(fake)
        self.assertFalse(runtime.supports_model_descriptor())
        with self.assertRaises(NativeRuntimeError):
            runtime.create_model_from_descriptor(
                ModelDefinition((Bone(None, (0, 0, 0)),))
            )

    def test_carrier_is_alive_during_call_and_can_be_collected_after_native_copy(
        self,
    ) -> None:
        fake = _FakeLibrary(feature_flags=FEATURE_MODEL_DESCRIPTOR)
        runtime = self._runtime(fake)
        definition = ModelDefinition(
            bones=(Bone(None, (0, 0, 0)), Bone(0, (0, 2, 0))),
            ik_solvers=(IkSolver(1, 0, (IkLink(0),)),),
            append_transforms=(AppendTransform(1, 0, 0.5, affect_rotation=True),),
            morph_count=1,
            bone_morph_offsets=(BoneMorphOffset(0, 1, (1, 0, 0), (0, 0, 0, 1)),),
            group_morph_offsets=(GroupMorphOffset(0, 0, 1.0),),
        )
        carrier = marshal_model_definition(definition)
        carrier_ref = weakref.ref(carrier)
        import mmd_anim._runtime as runtime_module

        original_marshal = runtime_module.marshal_model_definition
        runtime_module.marshal_model_definition = lambda _definition, _carrier=carrier: (
            _carrier
        )
        try:
            model = runtime.create_model_from_descriptor(definition)
        finally:
            runtime_module.marshal_model_definition = original_marshal
        self.assertEqual(fake.constructor_calls, 1)
        self.assertEqual(fake.observed, (2, 1, 1))
        self.assertEqual(
            fake.observed_payload,
            {
                "bones": (-1, 2.0),
                "ik_solvers": (1, 0, 1),
                "ik_links": (0, 0),
                "append_transforms": (1, 0),
                "bone_morph_offsets": (0, 1),
                "group_morph_offsets": (0, 0),
            },
        )
        del carrier
        gc.collect()
        self.assertIsNone(carrier_ref())
        model.close()
        self.assertEqual(fake.model_free_calls, 1)
        del model
        gc.collect()

    def test_null_handle_copies_indexed_last_error_immediately(self) -> None:
        fake = _FakeLibrary(feature_flags=FEATURE_MODEL_DESCRIPTOR)
        fake._create = lambda pointer: 0  # type: ignore[method-assign]
        fake.mmd_runtime_model_create_from_descriptor = fake._create
        runtime = self._runtime(fake)
        with self.assertRaisesRegex(NativeRuntimeError, r"bones\[3\]\.parent"):
            runtime.create_model_from_descriptor(
                ModelDefinition((Bone(None, (0, 0, 0)),))
            )


@unittest.skipUnless(
    os.environ.get("MMD_RUNTIME_LIBRARY")
    or (PYTHON_BINDING_ROOT.parents[1] / "target" / "release").is_dir(),
    "release cdylib not found; build mmd-anim-ffi --release to run native smoke",
)
class NativeModelDescriptorSmoke(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        from mmd_anim._runtime import local_release_library_path

        configured = os.environ.get("MMD_RUNTIME_LIBRARY")
        path = Path(configured) if configured else local_release_library_path()
        if not path.is_file():
            raise unittest.SkipTest(f"native library absent: {path}")
        cls.runtime = RuntimeLibrary(path)

    @staticmethod
    def maya_to_pmx(
        position_xyz: tuple[float, float, float],
    ) -> tuple[float, float, float]:
        x, y, z = position_xyz
        return x, y, -z

    def test_descriptor_model_rest_pose_ownership_and_name_map_error(self) -> None:
        definition = ModelDefinition(
            bones=(
                Bone(None, self.maya_to_pmx((0.0, 0.0, 0.0))),
                Bone(0, self.maya_to_pmx((1.0, 2.0, 3.0))),
                Bone(1, self.maya_to_pmx((4.0, 5.0, 6.0))),
            ),
            morph_count=0,
        )
        model = self.runtime.create_model_from_descriptor(definition)
        del definition
        gc.collect()
        self.assertEqual(model.bone_count(), 3)
        self.assertEqual(model.morph_count(), 0)
        instance = model.create_instance_for_model()
        try:
            instance.evaluate_rest_pose()
            world = instance.world_matrices_f32()
            skinning = instance.skinning_matrices_f32()
            self.assertTrue(all(math.isfinite(value) for value in world))
            self.assertTrue(all(math.isfinite(value) for value in skinning))
            self.assertEqual(len(skinning), 48)
            for matrix_index in range(3):
                matrix = skinning[matrix_index * 16 : (matrix_index + 1) * 16]
                for element_index, value in enumerate(matrix):
                    expected = 1.0 if element_index in (0, 5, 10, 15) else 0.0
                    self.assertAlmostEqual(value, expected, delta=1.0e-6)
            self.assertEqual((world[12], world[13], world[14]), (0.0, 0.0, 0.0))
            self.assertEqual((world[28], world[29], world[30]), (1.0, 2.0, -3.0))
            self.assertEqual((world[44], world[45], world[46]), (4.0, 5.0, -6.0))

            vmd = (
                PYTHON_BINDING_ROOT.parents[1]
                / "crates"
                / "mmd-anim-format"
                / "fixtures"
                / "vmd"
                / "ik_multi_bone_nondefault.vmd"
            ).read_bytes()
            with self.assertRaisesRegex(
                NativeRuntimeError,
                "clip_create_from_vmd_bytes_for_model failed: clip create failed",
            ):
                self.runtime.create_clip_from_vmd_bytes(model, vmd)
        finally:
            instance.close()
            model.close()
            model.close()


if __name__ == "__main__":
    unittest.main()
