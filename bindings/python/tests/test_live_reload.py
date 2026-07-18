from __future__ import annotations

import ctypes
import os
import sys
import threading
import unittest
import warnings
from pathlib import Path


PYTHON_BINDING_ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(PYTHON_BINDING_ROOT))

from mmd_anim._abi import MODEL_DESCRIPTOR_MANIFEST, ModelDescriptor  # noqa: E402
from mmd_anim._live_reload import (  # noqa: E402
    PHYSICS_FRAME_ACTION_SEED,
    PHYSICS_RIGIDBODY_MODE_DYNAMIC,
    HostPose,
    LiveRuntime,
    PhysicsDefinition,
    PhysicsRigidbody,
)
from mmd_anim._model_descriptor import (  # noqa: E402
    Bone,
    BoneMorphOffset,
    ModelDefinition,
)
from mmd_anim._runtime import (  # noqa: E402
    FEATURE_HOST_POSE_NATIVE_MORPHS,
    NativeRuntimeError,
    RuntimeLibrary,
)


FEATURE_MODEL_DESCRIPTOR = int(MODEL_DESCRIPTOR_MANIFEST["feature"]["value"])
FEATURE_PHYSICS = 1 << 1


def _one_bone() -> ModelDefinition:
    return ModelDefinition((Bone(None, (0.0, 0.0, 0.0)),))


def _two_bones() -> ModelDefinition:
    return ModelDefinition(
        (Bone(None, (0.0, 0.0, 0.0)), Bone(0, (0.0, 1.0, 0.0))),
        morph_count=1,
        bone_morph_offsets=(
            BoneMorphOffset(0, 1, (2.0, 0.0, 0.0), (0.0, 0.0, 0.0, 1.0)),
        ),
    )


def _pose(bones: int, morphs: int = 0) -> HostPose:
    return HostPose.for_model(bones, morphs)


class FakeLiveLibrary:
    def __init__(self, *, fail: str | None = None) -> None:
        self.fail = fail
        self.next_handle = 100
        self.events: list[tuple[str, int]] = []
        self.models: dict[int, tuple[int, int, int]] = {}
        self.instances: dict[int, int] = {}
        self.world_bindings: dict[int, list[int]] = {}
        self.last_error = b"fake live reload failure"
        self.block_create = False
        self.create_entered = threading.Event()
        self.allow_create = threading.Event()
        self.fail_close = False
        self.mmd_runtime_model_create_from_descriptor = self._model_create

    def _handle(self) -> int:
        self.next_handle += 1
        return self.next_handle

    def mmd_runtime_abi_version(self) -> int:
        return 2

    def mmd_runtime_feature_flags(self) -> int:
        return (
            FEATURE_MODEL_DESCRIPTOR
            | FEATURE_PHYSICS
            | FEATURE_HOST_POSE_NATIVE_MORPHS
        )

    def mmd_runtime_last_error_message(self) -> bytes:
        return self.last_error

    def _model_create(self, descriptor_pointer: object) -> int:
        if self.fail == "model":
            return 0
        if self.block_create:
            self.create_entered.set()
            self.allow_create.wait(timeout=5)
        descriptor = ctypes.cast(
            descriptor_pointer, ctypes.POINTER(ModelDescriptor)
        ).contents
        handle = self._handle()
        self.models[handle] = (
            descriptor.bone_count,
            descriptor.morph_count,
            descriptor.ik_solver_count,
        )
        self.events.append(("model_create", handle))
        return handle

    def mmd_runtime_model_bone_count(self, handle: int) -> int:
        return self.models[handle][0]

    def mmd_runtime_model_morph_count(self, handle: int) -> int:
        return self.models[handle][1]

    def mmd_runtime_model_ik_count(self, handle: int) -> int:
        return self.models[handle][2]

    def mmd_runtime_model_free(self, handle: int) -> None:
        self.events.append(("model_free", handle))
        self.models.pop(handle, None)
        if self.fail_close:
            raise RuntimeError("fake old model cleanup failure")

    def mmd_runtime_instance_create_for_model(self, model: int) -> int:
        if self.fail == "instance":
            return 0
        handle = self._handle()
        self.instances[handle] = model
        self.events.append(("instance_create", handle))
        return handle

    def mmd_runtime_instance_free(self, handle: int) -> None:
        self.events.append(("instance_free", handle))
        self.instances.pop(handle, None)

    def mmd_runtime_instance_apply_host_pose_and_evaluate_before_physics(
        self, instance: int, view: object
    ) -> int:
        del instance, view
        return 1 if self.fail == "pose" else 0

    def mmd_runtime_instance_apply_host_pose(self, instance: int, view: object) -> int:
        del instance, view
        return 1 if self.fail == "pose" else 0

    def mmd_runtime_instance_evaluate_current_pose_after_physics(
        self, instance: int
    ) -> int:
        del instance
        return 0

    def mmd_runtime_physics_world_create(
        self,
        rigidbodies: object,
        rigidbody_count: int,
        joints: object,
        joint_count: int,
        out_world: object,
    ) -> int:
        del joints, joint_count
        if self.fail == "world":
            return 1
        handle = self._handle()
        bindings = [rigidbodies[i].bone_index for i in range(rigidbody_count)]
        self.world_bindings[handle] = bindings
        ctypes.cast(out_world, ctypes.POINTER(ctypes.c_void_p)).contents.value = handle
        self.events.append(("world_create", handle))
        return 0

    def mmd_runtime_physics_world_free(self, world: int) -> None:
        self.events.append(("world_free", world))
        self.world_bindings.pop(world, None)

    def mmd_runtime_physics_world_rigidbody_count(
        self, world: int, out_count: object
    ) -> int:
        ctypes.cast(out_count, ctypes.POINTER(ctypes.c_size_t)).contents.value = len(
            self.world_bindings[world]
        )
        return 0

    def mmd_runtime_physics_world_copy_rigidbody_bindings(
        self, world: int, output: object, capacity: int, out_count: object
    ) -> int:
        bindings = self.world_bindings[world]
        for index, bone in enumerate(bindings[:capacity]):
            output[index].bone_index = bone
            output[index].mode = 0
        ctypes.cast(out_count, ctypes.POINTER(ctypes.c_size_t)).contents.value = len(
            bindings
        )
        return 0

    def mmd_runtime_evaluate_host_frame(
        self,
        instance: int,
        world: int,
        pose: object,
        action: int,
        dt_seconds: float,
        ik_tolerance: float,
        ik_cap: int,
        report: object,
    ) -> int:
        del pose, action, dt_seconds, ik_tolerance, ik_cap, report
        bone_count = self.models[self.instances[instance]][0]
        if any(index >= bone_count for index in self.world_bindings[world]):
            for rigidbody_index, bone_index in enumerate(self.world_bindings[world]):
                if bone_index >= bone_count:
                    self.last_error = (
                        f"physics_world.rigidbodies[{rigidbody_index}].bone_index: "
                        f"{bone_index} exceeds instance bone_count {bone_count}"
                    ).encode()
                    break
            return 1
        if self.fail == "seed":
            return 1
        return 0


def _runtime(fake: FakeLiveLibrary) -> RuntimeLibrary:
    runtime = object.__new__(RuntimeLibrary)
    runtime._lib = fake
    return runtime


class FakeLiveReloadTests(unittest.TestCase):
    def test_requires_native_host_pose_morph_capability(self) -> None:
        fake = FakeLiveLibrary()
        fake.mmd_runtime_feature_flags = lambda: FEATURE_MODEL_DESCRIPTOR  # type: ignore[method-assign]
        with self.assertRaisesRegex(NativeRuntimeError, "native HostPose"):
            LiveRuntime(_runtime(fake))

    def test_reload_serializes_close_and_terminal_close_rejects_reload(self) -> None:
        fake = FakeLiveLibrary()
        runtime = _runtime(fake)
        live = LiveRuntime(runtime, _one_bone(), _pose(1))
        fake.block_create = True
        reload_done = threading.Event()

        def do_reload() -> None:
            live.reload(_two_bones(), _pose(2, 1))
            reload_done.set()

        reload_thread = threading.Thread(target=do_reload)
        reload_thread.start()
        self.assertTrue(fake.create_entered.wait(timeout=2))
        close_done = threading.Event()
        close_thread = threading.Thread(target=lambda: (live.close(), close_done.set()))
        close_thread.start()
        self.assertFalse(close_done.wait(timeout=0.05))
        fake.allow_create.set()
        reload_thread.join(timeout=2)
        close_thread.join(timeout=2)
        self.assertTrue(reload_done.is_set())
        self.assertTrue(close_done.is_set())
        with self.assertRaisesRegex(NativeRuntimeError, "closed"):
            live.reload(_one_bone(), _pose(1))

    def test_successful_swap_is_atomic_and_closes_old_world_instance_model(
        self,
    ) -> None:
        fake = FakeLiveLibrary()
        runtime = _runtime(fake)
        live = LiveRuntime(
            runtime,
            _one_bone(),
            _pose(1),
            PhysicsDefinition((PhysicsRigidbody(bone_index=0),)),
        )
        old = live.handle_set
        fake.models[old.model._handle] = (1, 0, 0)
        old_ids = (old.model._handle, old.instance._handle)

        live.reload(
            _two_bones(),
            _pose(2, 1),
            PhysicsDefinition((PhysicsRigidbody(bone_index=0),)),
        )
        current = live.handle_set
        self.assertIsNot(current, old)
        self.assertEqual((old.model._handle, old.instance._handle), (None, None))
        self.assertEqual(
            [event[0] for event in fake.events[-3:]],
            ["world_free", "instance_free", "model_free"],
        )
        self.assertEqual(old_ids[0] not in fake.models, True)
        live.close()

    def test_omitted_physics_preserves_and_none_disables(self) -> None:
        fake = FakeLiveLibrary()
        runtime = _runtime(fake)
        physics = PhysicsDefinition((PhysicsRigidbody(bone_index=0),))
        live = LiveRuntime(runtime, _one_bone(), _pose(1), physics)

        live.reload(_two_bones(), _pose(2, 1))
        self.assertIsNotNone(live.physics_world)
        live.reload(_one_bone(), _pose(1), None)
        self.assertIsNone(live.physics_world)
        live.close()

    def test_swap_keeps_new_generation_when_old_cleanup_raises(self) -> None:
        fake = FakeLiveLibrary()
        runtime = _runtime(fake)
        live = LiveRuntime(runtime, _one_bone(), _pose(1))
        fake.fail_close = True
        with warnings.catch_warnings(record=True) as caught:
            warnings.simplefilter("always", ResourceWarning)
            live.reload(_two_bones(), _pose(2, 1))
        self.assertEqual(live.model.bone_count(), 2)
        self.assertTrue(
            any("old handle cleanup failed" in str(item.message) for item in caught)
        )
        fake.fail_close = False
        live.close()

    def test_candidate_failure_frees_world_instance_model_in_reverse_order(
        self,
    ) -> None:
        for failure, expected in (
            ("instance", ["model_free"]),
            ("world", ["instance_free", "model_free"]),
            ("seed", ["world_free", "instance_free", "model_free"]),
        ):
            with self.subTest(failure=failure):
                fake = FakeLiveLibrary()
                runtime = _runtime(fake)
                live = LiveRuntime(runtime, _one_bone(), _pose(1))
                old = live.handle_set
                fake.fail = failure
                with self.assertRaises(NativeRuntimeError):
                    live.reload(
                        _two_bones(),
                        _pose(2, 1),
                        PhysicsDefinition((PhysicsRigidbody(bone_index=0),)),
                    )
                self.assertIs(live.handle_set, old)
                self.assertEqual(
                    [event[0] for event in fake.events[-len(expected) :]], expected
                )
                live.close()

    def test_invalid_physics_binding_preserves_old_handle_set(self) -> None:
        fake = FakeLiveLibrary()
        runtime = _runtime(fake)
        live = LiveRuntime(runtime, _one_bone(), _pose(1))
        old = live.handle_set
        with self.assertRaisesRegex(
            NativeRuntimeError,
            r"physics_world\.rigidbodies\[0\]\.bone_index: 1 exceeds instance bone_count 1",
        ):
            live.reload(
                _one_bone(),
                _pose(1),
                PhysicsDefinition((PhysicsRigidbody(bone_index=1),)),
            )
        self.assertIs(live.handle_set, old)
        self.assertIsNotNone(old.model._handle)
        live.close()


@unittest.skipUnless(
    os.environ.get("MMD_RUNTIME_LIBRARY")
    or (PYTHON_BINDING_ROOT.parents[1] / "target" / "release").is_dir(),
    "release cdylib not found; build mmd-anim-ffi --release to run native smoke",
)
class NativeLiveReloadTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        from mmd_anim._runtime import local_release_library_path

        configured = os.environ.get("MMD_RUNTIME_LIBRARY")
        path = Path(configured) if configured else local_release_library_path()
        if not path.is_file():
            raise unittest.SkipTest(f"native library absent: {path}")
        cls.runtime = RuntimeLibrary(path)

    def test_no_physics_reload_changes_counts_pose_and_closes_old_generation(
        self,
    ) -> None:
        independent_model = self.runtime.create_model_from_descriptor(_one_bone())
        independent_instance = independent_model.create_instance_for_model()
        independent_instance.evaluate_rest_pose()
        independent_world_before = list(independent_instance.world_matrices_f32())
        independent_morph_before = list(independent_instance.morph_weights_f32())
        live = LiveRuntime(self.runtime, _one_bone(), _pose(1))
        old_model = live.model
        old_instance = live.instance
        live.reload(_two_bones(), _pose(2, 1))
        self.assertEqual((live.model.bone_count(), live.model.morph_count()), (2, 1))
        self.assertIsNone(old_model._handle)
        self.assertIsNone(old_instance._handle)
        zero_morph_pose = HostPose(
            local_position_offsets_xyz=(0.0, 0.0, 0.0, 1.0, 0.0, 0.0),
            local_rotation_xyzw=(0.0, 0.0, 0.0, 1.0) * 2,
            local_scales_xyz=(1.0, 1.0, 1.0) * 2,
            morph_weights=(0.0,),
        )
        live.evaluate_host_frame(zero_morph_pose)
        self.assertEqual(len(live.instance.morph_weights_f32()), 1)
        world_zero = live.instance.world_matrices_f32()
        self.assertAlmostEqual(world_zero[28], 1.0, places=6)
        one_morph_pose = HostPose(
            local_position_offsets_xyz=(0.0, 0.0, 0.0, 1.0, 0.0, 0.0),
            local_rotation_xyzw=(0.0, 0.0, 0.0, 1.0) * 2,
            local_scales_xyz=(1.0, 1.0, 1.0) * 2,
            morph_weights=(1.0,),
        )
        live.evaluate_host_frame(one_morph_pose)
        self.assertEqual(live.instance.morph_weights_f32().tolist(), [1.0])
        world_morph = live.instance.world_matrices_f32()
        self.assertAlmostEqual(world_morph[28], 3.0, places=6)
        self.assertEqual(
            independent_world_before, list(independent_instance.world_matrices_f32())
        )
        self.assertEqual(
            independent_morph_before, list(independent_instance.morph_weights_f32())
        )
        live.close()
        independent_instance.close()
        independent_model.close()

    def test_invalid_descriptor_preserves_old_generation(self) -> None:
        live = LiveRuntime(self.runtime, _one_bone(), _pose(1))
        old = live.handle_set
        with self.assertRaises(NativeRuntimeError):
            live.reload(ModelDefinition((Bone(4, (0.0, 0.0, 0.0)),)), _pose(1))
        self.assertIs(live.handle_set, old)
        self.assertIsNotNone(live.model._handle)
        live.close()

    def test_physics_binding_mismatch_is_indexed_and_candidate_is_freed(self) -> None:
        if not self.runtime.supports_native_physics():
            self.skipTest("release cdylib lacks Bullet physics")
        live = LiveRuntime(self.runtime, _one_bone(), _pose(1))
        old = live.handle_set
        old_world = list(old.instance.world_matrices_f32())
        with self.assertRaisesRegex(
            NativeRuntimeError,
            r"physics_world\.rigidbodies\[0\]\.bone_index: 1 exceeds instance bone_count 1",
        ):
            live.reload(
                _one_bone(),
                _pose(1),
                PhysicsDefinition((PhysicsRigidbody(bone_index=1),)),
            )
        self.assertIs(live.handle_set, old)
        self.assertIsNotNone(old.model._handle)
        old.instance.apply_host_pose_and_evaluate_before_physics(_pose(1))
        old.instance.evaluate_current_pose_after_physics()
        self.assertEqual(old_world, list(old.instance.world_matrices_f32()))
        live.close()

    def test_physics_reload_fresh_world_seed_has_no_continuity_contract(self) -> None:
        if not self.runtime.supports_native_physics():
            self.skipTest("release cdylib lacks Bullet physics")
        physics = PhysicsDefinition(
            (
                PhysicsRigidbody(
                    bone_index=0,
                    mode=PHYSICS_RIGIDBODY_MODE_DYNAMIC,
                    position_xyz=(0.0, 4.0, 0.0),
                ),
            )
        )
        pose = HostPose(
            local_position_offsets_xyz=(0.0, 4.0, 0.0),
            local_rotation_xyzw=(0.0, 0.0, 0.0, 1.0),
            local_scales_xyz=(1.0, 1.0, 1.0),
        )
        live = LiveRuntime(self.runtime, _one_bone(), pose, physics)
        old_world = live.physics_world
        self.assertEqual(old_world.rigidbody_count(), 1)
        self.assertEqual(old_world.rigidbody_bindings()[0].bone_index, 0)
        self.assertEqual(
            old_world.rigidbody_bindings()[0].mode, PHYSICS_RIGIDBODY_MODE_DYNAMIC
        )
        checkpoint_one = list(live.instance.world_matrices_f32())
        state_one = list(old_world.rigidbody_states_f32())
        self.assertAlmostEqual(checkpoint_one[13], 4.0, places=6)
        self.assertAlmostEqual(state_one[1], 4.0, places=6)

        seed_report = live.evaluate_host_frame(pose, action=PHYSICS_FRAME_ACTION_SEED)
        self.assertEqual(seed_report.tick.substeps, 0)
        self.assertEqual(seed_report.kinematic_rigidbodies_fed, 0)
        self.assertEqual(seed_report.bones_written_back, 0)

        live.reload(_one_bone(), pose, physics)
        self.assertIsNot(live.physics_world, old_world)
        self.assertIsNone(old_world._handle)
        self.assertEqual(live.physics_world.rigidbody_count(), 1)
        self.assertEqual(live.physics_world.rigidbody_bindings()[0].bone_index, 0)
        self.assertEqual(
            live.physics_world.rigidbody_bindings()[0].mode,
            PHYSICS_RIGIDBODY_MODE_DYNAMIC,
        )
        checkpoint_two = list(live.instance.world_matrices_f32())
        state_two = list(live.physics_world.rigidbody_states_f32())
        self.assertEqual(checkpoint_one, checkpoint_two)
        self.assertEqual(state_one, state_two)
        live.close()


if __name__ == "__main__":
    unittest.main()
