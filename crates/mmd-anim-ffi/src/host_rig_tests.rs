use super::*;

#[test]
fn host_rig_abi_preserves_pose_and_reuses_buffers_after_model_free() {
    let model_arena = Arc::new(
        ModelArena::new_full(
            vec![
                mmd_anim_runtime::BoneInit::new(None, glam::Vec3A::ZERO),
                mmd_anim_runtime::BoneInit::new(Some(BoneIndex(0)), glam::Vec3A::Y),
                mmd_anim_runtime::BoneInit::new(Some(BoneIndex(1)), glam::Vec3A::Y),
                mmd_anim_runtime::BoneInit::new(None, glam::Vec3A::ZERO),
            ],
            vec![],
            vec![
                mmd_anim_runtime::AppendTransformInit::new(BoneIndex(1), BoneIndex(0), 0.5)
                    .with_rotation(),
                mmd_anim_runtime::AppendTransformInit::new(BoneIndex(2), BoneIndex(1), 0.7)
                    .with_rotation(),
                mmd_anim_runtime::AppendTransformInit::new(BoneIndex(3), BoneIndex(2), 1.0)
                    .with_rotation(),
            ],
        )
        .unwrap(),
    );
    let model = Box::into_raw(Box::new(MmdRuntimeModel {
        model: model_arena.clone(),
        bone_name_to_index: HashMap::new(),
        morph_name_to_index: HashMap::new(),
        ik_solver_bone_name_to_index: HashMap::new(),
    }));
    let instance = unsafe { mmd_runtime_instance_create(model, 0) };
    let driven = [0, 2];
    let rig = unsafe { mmd_runtime_host_rig_create(model, driven.as_ptr(), 2, ptr::null(), 0) };
    assert!(!rig.is_null());
    assert_ne!(
        mmd_runtime_feature_flags() & MMD_RUNTIME_FEATURE_HOST_RIG,
        0
    );
    unsafe { mmd_runtime_model_free(model) };
    let positions = [0.0; 12];
    let mut rotations = [0.0; 16];
    let scales = [1.0; 12];
    for index in 0..4 {
        rotations[index * 4 + 3] = 1.0;
    }
    rotations[0..4].copy_from_slice(&glam::Quat::from_rotation_z(0.4).to_array());
    rotations[8..12].copy_from_slice(&glam::Quat::from_rotation_z(0.2).to_array());
    let mut view = MmdRuntimeFfiHostPoseView {
        local_position_offsets_xyz: positions.as_ptr(),
        local_rotation_xyzw: rotations.as_ptr(),
        local_scales_xyz: scales.as_ptr(),
        bone_count: 4,
        morph_weights: ptr::null(),
        morph_count: 0,
        ik_enabled: ptr::null(),
        ik_count: 0,
    };
    let mut out = [0.0; 64];
    let buffer_address = unsafe { (*rig).positions.as_ptr() };
    for _ in 0..3 {
        assert_eq!(
            unsafe { mmd_runtime_instance_evaluate_host_rig_pose(instance, rig, &view, 1e-4, 0) },
            MmdRuntimeStatus::Ok
        );
        assert!(unsafe {
            mmd_runtime_instance_copy_world_matrices(instance, out.as_mut_ptr(), out.len())
        });
        assert_eq!(buffer_address, unsafe { (*rig).positions.as_ptr() });
        // Analytic FK oracle, shared numerically with the real WASM harness.
        assert!((out[32 + 12] + 2.0 * 0.4f32.sin()).abs() < 1e-5);
        assert!((out[32 + 13] - 2.0 * 0.4f32.cos()).abs() < 1e-5);
        assert!((out[32] - 0.6f32.cos()).abs() < 1e-5);
        assert!((out[48] - 0.2f32.cos()).abs() < 1e-5);
    }
    let mut reference = RuntimeInstance::new(model_arena.clone());
    let core_rig =
        mmd_anim_runtime::HostRigDefinition::new(model_arena, &[BoneIndex(0), BoneIndex(2)], &[])
            .unwrap();
    let r = unsafe { &*rig };
    reference
        .evaluate_host_rig_pose(
            &core_rig,
            &HostPoseView {
                local_position_offsets: &r.positions,
                local_rotations: &r.rotations,
                local_scales: &r.scales,
                morph_weights: &[],
                ik_enabled: &[],
            },
            IkSolveOptions::default(),
        )
        .unwrap();
    let expected: Vec<_> = reference
        .world_matrices()
        .iter()
        .flat_map(|m| m.to_cols_array())
        .collect();
    assert_eq!(&out[..], &expected);
    let before = out;
    // No malformed count may cause a read of an input array.
    view.bone_count = usize::MAX;
    view.local_position_offsets_xyz = ptr::null();
    assert_eq!(
        unsafe { mmd_runtime_instance_evaluate_host_rig_pose(instance, rig, &view, 1e-4, 0) },
        MmdRuntimeStatus::InvalidInput
    );
    view.bone_count = 4;
    // Valid allocation with deliberately misaligned pointer: rejected before dereference.
    view.local_position_offsets_xyz = unsafe { positions.as_ptr().cast::<u8>().add(1).cast() };
    assert_eq!(
        unsafe { mmd_runtime_instance_evaluate_host_rig_pose(instance, rig, &view, 1e-4, 0) },
        MmdRuntimeStatus::InvalidInput
    );
    view.local_position_offsets_xyz = positions.as_ptr();
    rotations[0] = f32::NAN;
    assert_eq!(
        unsafe { mmd_runtime_instance_evaluate_host_rig_pose(instance, rig, &view, 1e-4, 0) },
        MmdRuntimeStatus::InvalidInput
    );
    assert!(unsafe {
        mmd_runtime_instance_copy_world_matrices(instance, out.as_mut_ptr(), out.len())
    });
    assert_eq!(out, before);
    rotations[0] = 0.0;
    view.local_rotation_xyzw = rotations.as_ptr();
    assert_eq!(
        unsafe { mmd_runtime_instance_evaluate_host_rig_pose(instance, rig, &view, 1e-4, 0) },
        MmdRuntimeStatus::Ok
    );
    unsafe {
        mmd_runtime_host_rig_free(rig);
        mmd_runtime_instance_free(instance);
    }
}

#[test]
fn host_rig_abi_rejects_invalid_creation_and_handles() {
    unsafe {
        assert!(mmd_runtime_host_rig_create(ptr::null(), ptr::null(), 0, ptr::null(), 0).is_null());
        let model = mmd_runtime_model_create([-1].as_ptr(), [0.0; 3].as_ptr(), 1);
        for driven in [&[1u32][..], &[0u32, 0][..]] {
            assert!(
                mmd_runtime_host_rig_create(model, driven.as_ptr(), driven.len(), ptr::null(), 0)
                    .is_null()
            );
            assert!(!mmd_runtime_last_error_message().is_null());
        }
        assert!(mmd_runtime_host_rig_create(model, ptr::null(), 1, ptr::null(), 0).is_null());
        assert!(mmd_runtime_host_rig_create(model, ptr::null(), 0, [0].as_ptr(), 1).is_null());
        assert_eq!(
            mmd_runtime_instance_evaluate_host_rig_pose(
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null(),
                1e-4,
                0
            ),
            MmdRuntimeStatus::InvalidInput
        );
        mmd_runtime_model_free(model);
        mmd_runtime_host_rig_free(ptr::null_mut());
    }
}
