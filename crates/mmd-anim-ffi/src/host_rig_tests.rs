use super::*;

struct ExternalPhysicsCallbackState {
    calls: usize,
    status: u32,
    write_non_finite: bool,
}

unsafe extern "C" fn external_physics_callback(
    user_data: *mut c_void,
    before_world_matrices_f32: *const f32,
    before_world_matrices_f32_len: usize,
    physics_world_matrices_f32: *mut f32,
    physics_world_matrices_f32_len: usize,
    physics_world_matrix_mask_u8: *mut u8,
    physics_world_matrix_mask_u8_len: usize,
) -> u32 {
    assert_eq!(before_world_matrices_f32_len, 32);
    assert_eq!(physics_world_matrices_f32_len, 32);
    assert_eq!(physics_world_matrix_mask_u8_len, 2);
    let state = unsafe { &mut *user_data.cast::<ExternalPhysicsCallbackState>() };
    state.calls += 1;
    if state.status != MmdRuntimeStatus::Ok as u32 {
        return state.status;
    }
    let before =
        unsafe { slice::from_raw_parts(before_world_matrices_f32, before_world_matrices_f32_len) };
    let physics = unsafe {
        slice::from_raw_parts_mut(physics_world_matrices_f32, physics_world_matrices_f32_len)
    };
    let mask = unsafe {
        slice::from_raw_parts_mut(
            physics_world_matrix_mask_u8,
            physics_world_matrix_mask_u8_len,
        )
    };
    assert_eq!(before, physics);
    physics[12] = 99.0;
    physics[16 + 13] -= 0.25;
    if state.write_non_finite {
        physics[16] = f32::NAN;
    }
    mask.copy_from_slice(&[1, 1]);
    MmdRuntimeStatus::Ok as u32
}

#[test]
fn host_rig_external_physics_callback_is_backend_neutral_and_recovers() {
    let parents = [-1, 0];
    let rest_positions = [0.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    let model = unsafe {
        mmd_runtime_model_create(parents.as_ptr(), rest_positions.as_ptr(), parents.len())
    };
    let instance = unsafe { mmd_runtime_instance_create_for_model(model) };
    let driven = [0u32];
    let rig = unsafe {
        mmd_runtime_host_rig_create(model, driven.as_ptr(), driven.len(), ptr::null(), 0)
    };
    assert!(!model.is_null() && !instance.is_null() && !rig.is_null());
    assert_ne!(
        mmd_runtime_feature_flags() & MMD_RUNTIME_FEATURE_HOST_RIG_EXTERNAL_PHYSICS,
        0
    );

    let positions = [2.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    let rotations = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    let scales = [1.0f32; 6];
    let view = MmdRuntimeFfiHostPoseView {
        local_position_offsets_xyz: positions.as_ptr(),
        local_rotation_xyzw: rotations.as_ptr(),
        local_scales_xyz: scales.as_ptr(),
        bone_count: 2,
        morph_weights: ptr::null(),
        morph_count: 0,
        ik_enabled: ptr::null(),
        ik_count: 0,
    };
    let mut state = ExternalPhysicsCallbackState {
        calls: 0,
        status: MmdRuntimeStatus::Error as u32,
        write_non_finite: false,
    };
    assert_eq!(
        unsafe {
            mmd_runtime_evaluate_host_rig_frame_with_external_physics(
                instance,
                rig,
                &view,
                1.0e-4,
                0,
                Some(external_physics_callback),
                (&mut state as *mut ExternalPhysicsCallbackState).cast(),
            )
        },
        MmdRuntimeStatus::Error
    );
    let cached_before =
        unsafe { slice::from_raw_parts(mmd_runtime_instance_world_matrices(instance), 32) };
    assert!((cached_before[12] - 2.0).abs() < 1.0e-5);
    assert!((cached_before[16 + 12] - 2.0).abs() < 1.0e-5);
    assert!((cached_before[16 + 13] - 1.0).abs() < 1.0e-5);
    state.status = MmdRuntimeStatus::Ok as u32;
    assert_eq!(
        unsafe {
            mmd_runtime_evaluate_host_rig_frame_with_external_physics(
                instance,
                rig,
                &view,
                1.0e-4,
                0,
                Some(external_physics_callback),
                (&mut state as *mut ExternalPhysicsCallbackState).cast(),
            )
        },
        MmdRuntimeStatus::Ok
    );
    assert_eq!(state.calls, 2);
    let mut matrices = [0.0f32; 32];
    assert!(unsafe {
        mmd_runtime_instance_copy_world_matrices(instance, matrices.as_mut_ptr(), matrices.len())
    });
    assert!((matrices[12] - 2.0).abs() < 1.0e-5);
    assert!((matrices[16 + 12] - 2.0).abs() < 1.0e-5);
    assert!((matrices[16 + 13] - 0.75).abs() < 1.0e-5);

    state.write_non_finite = true;
    assert_eq!(
        unsafe {
            mmd_runtime_evaluate_host_rig_frame_with_external_physics(
                instance,
                rig,
                &view,
                1.0e-4,
                0,
                Some(external_physics_callback),
                (&mut state as *mut ExternalPhysicsCallbackState).cast(),
            )
        },
        MmdRuntimeStatus::InvalidInput
    );
    assert_eq!(
        unsafe {
            mmd_runtime_evaluate_host_rig_frame_with_external_physics(
                instance,
                rig,
                &view,
                1.0e-4,
                0,
                None,
                ptr::null_mut(),
            )
        },
        MmdRuntimeStatus::InvalidInput
    );

    unsafe {
        mmd_runtime_host_rig_free(rig);
        mmd_runtime_instance_free(instance);
        mmd_runtime_model_free(model);
    }
}

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

#[cfg(feature = "physics-bullet-native")]
#[test]
fn host_rig_physics_keeps_driven_parent_and_writes_dynamic_child() {
    let parents = [-1, 0];
    let rest_positions = [0.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    let model = unsafe {
        mmd_runtime_model_create(parents.as_ptr(), rest_positions.as_ptr(), parents.len())
    };
    assert!(!model.is_null());
    let instance = unsafe { mmd_runtime_instance_create_for_model(model) };
    assert!(!instance.is_null());
    assert!(unsafe { mmd_runtime_instance_evaluate_rest_pose(instance) });
    assert_eq!(
        unsafe {
            mmd_runtime_instance_set_physics_mode(instance, MmdRuntimeFfiPhysicsMode::Live as u32)
        },
        MmdRuntimeStatus::Ok
    );

    let body = MmdRuntimeFfiPhysicsRigidBodyDesc {
        shape: MmdRuntimeFfiPhysicsRigidBodyShape::Sphere as u32,
        shape_size: [0.1, 0.0, 0.0],
        position_xyz: [0.0, 1.0, 0.0],
        rotation_euler_xyz: [0.0; 3],
        mass: 1.0,
        linear_damping: 0.0,
        angular_damping: 0.0,
        friction: 0.5,
        restitution: 0.0,
        collision_group: 0,
        collision_mask: 0xffff,
        bone_index: 1,
        mode: MmdRuntimeFfiPhysicsRigidBodyMode::Dynamic as u32,
        body_from_bone_position_xyz: [0.0; 3],
        body_from_bone_rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
        bone_from_body_position_xyz: [0.0; 3],
        bone_from_body_rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
    };
    let mut world = ptr::null_mut();
    assert_eq!(
        unsafe { mmd_runtime_physics_world_create(&body, 1, ptr::null(), 0, &mut world,) },
        MmdRuntimeStatus::Ok
    );
    assert!(!world.is_null());

    let driven = [0u32];
    let rig = unsafe {
        mmd_runtime_host_rig_create(model, driven.as_ptr(), driven.len(), ptr::null(), 0)
    };
    assert!(!rig.is_null());
    assert_ne!(
        mmd_runtime_feature_flags() & MMD_RUNTIME_FEATURE_HOST_RIG_PHYSICS,
        0
    );

    let positions = [2.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    let rotations = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    let scales = [1.0f32; 6];
    let view = MmdRuntimeFfiHostPoseView {
        local_position_offsets_xyz: positions.as_ptr(),
        local_rotation_xyzw: rotations.as_ptr(),
        local_scales_xyz: scales.as_ptr(),
        bone_count: 2,
        morph_weights: ptr::null(),
        morph_count: 0,
        ik_enabled: ptr::null(),
        ik_count: 0,
    };
    assert_eq!(
        unsafe {
            mmd_runtime_evaluate_host_rig_frame(
                instance,
                world,
                rig,
                &view,
                99,
                0.0,
                1.0e-3,
                0,
                ptr::null_mut(),
            )
        },
        MmdRuntimeStatus::InvalidInput
    );
    let mut report = MmdRuntimeFfiPhysicsWorldStepReport {
        tick: MmdRuntimeFfiPhysicsStepStats {
            input_dt_seconds: 0.0,
            clamped_dt_seconds: 0.0,
            substeps: 0,
            accumulator_seconds: 0.0,
        },
        kinematic_rigidbodies_fed: 0,
        bones_written_back: 0,
    };
    assert_eq!(
        unsafe {
            mmd_runtime_evaluate_host_rig_frame(
                instance,
                world,
                rig,
                &view,
                MmdRuntimePhysicsFrameAction::Seed as u32,
                0.0,
                1.0e-3,
                0,
                &mut report,
            )
        },
        MmdRuntimeStatus::Ok
    );
    assert_eq!(report.tick.substeps, 0);

    let mut matrices = [0.0f32; 32];
    assert!(unsafe {
        mmd_runtime_instance_copy_world_matrices(instance, matrices.as_mut_ptr(), matrices.len())
    });
    assert!((matrices[12] - 2.0).abs() < 1.0e-4);
    assert!((matrices[28] - 2.0).abs() < 1.0e-4);
    let mut seeded_body = [0.0f32; 7];
    assert_eq!(
        unsafe {
            mmd_runtime_physics_world_copy_rigidbody_states(
                world,
                seeded_body.as_mut_ptr(),
                seeded_body.len(),
            )
        },
        MmdRuntimeStatus::Ok
    );

    assert_eq!(
        unsafe {
            mmd_runtime_evaluate_host_rig_frame(
                instance,
                world,
                rig,
                &view,
                MmdRuntimePhysicsFrameAction::Step as u32,
                1.0 / 60.0,
                1.0e-3,
                0,
                &mut report,
            )
        },
        MmdRuntimeStatus::Ok
    );
    assert_eq!(report.bones_written_back, 1);
    assert!(unsafe {
        mmd_runtime_instance_copy_world_matrices(instance, matrices.as_mut_ptr(), matrices.len())
    });
    assert!((matrices[12] - 2.0).abs() < 1.0e-4);
    assert!((matrices[28] - 2.0).abs() < 1.0e-4);
    assert!(
        matrices[29] < 1.0,
        "dynamic child should fall: {matrices:?}"
    );

    let mut stepped_body = [0.0f32; 7];
    assert_eq!(
        unsafe {
            mmd_runtime_physics_world_copy_rigidbody_states(
                world,
                stepped_body.as_mut_ptr(),
                stepped_body.len(),
            )
        },
        MmdRuntimeStatus::Ok
    );
    assert!(stepped_body[1] < seeded_body[1]);

    unsafe {
        mmd_runtime_physics_world_free(world);
        mmd_runtime_host_rig_free(rig);
        mmd_runtime_instance_free(instance);
        mmd_runtime_model_free(model);
    }
}
