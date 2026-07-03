use super::*;
use std::ffi::CStr;

fn last_error_cstr() -> Option<&'static CStr> {
    let ptr = mmd_runtime_last_error_message();
    if ptr.is_null() {
        None
    } else {
        Some(unsafe { CStr::from_ptr(ptr) })
    }
}

#[test]
fn last_error_message_is_null_when_no_error() {
    assert!(mmd_runtime_last_error_message().is_null());
}

#[test]
fn panic_guard_returns_default_and_sets_last_error() {
    assert!(!mmd_runtime_test_trigger_panic_guard());
    let message = last_error_cstr().expect("expected panic guard error message");
    assert_eq!(message.to_bytes(), FFI_PANIC_ERROR_MESSAGE.as_bytes());
}

#[test]
fn last_error_message_survives_read_without_clearing() {
    set_last_error("fixture error");
    let message = last_error_cstr().expect("expected stored error message");
    assert_eq!(message.to_bytes(), b"fixture error");
    let again = last_error_cstr().expect("last error should remain readable");
    assert_eq!(again.to_bytes(), b"fixture error");
}

#[test]
fn failing_ffi_parse_sets_last_error_message() {
    let garbage = [0u8; 16];
    let buf = unsafe { mmd_runtime_parse_vmd_json(garbage.as_ptr(), garbage.len()) };
    assert!(buf.data.is_null());
    assert_eq!(buf.len, 0);
    let message = last_error_cstr().expect("expected vmd parse error");
    assert_eq!(message.to_bytes(), FFI_ERR_VMD_PARSE_FAILED.as_bytes());

    let pmx_buf =
        unsafe { mmd_runtime_parse_pmx_non_geometry_json(garbage.as_ptr(), garbage.len()) };
    assert!(pmx_buf.data.is_null());
    let pmx_message = last_error_cstr().expect("expected pmx parse error");
    assert_eq!(pmx_message.to_bytes(), FFI_ERR_PMX_PARSE_FAILED.as_bytes());
}

#[test]
fn failing_ffi_import_sets_last_error_message() {
    let garbage = [0u8; 32];
    let model = unsafe { mmd_runtime_model_create_from_pmx_bytes(garbage.as_ptr(), garbage.len()) };
    assert!(model.is_null());
    let message = last_error_cstr().expect("expected pmx import error");
    assert_eq!(message.to_bytes(), FFI_ERR_PMX_IMPORT_FAILED.as_bytes());
}

#[test]
fn abi_version_matches_current_breaking_surface() {
    assert_eq!(ABI_VERSION, 2);
    assert_eq!(mmd_runtime_abi_version(), ABI_VERSION);
}

fn assert_near(actual: f32, expected: f32, tolerance: f32) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "actual={actual} expected={expected} tolerance={tolerance}"
    );
}

fn assert_slice_near(actual: &[f32], expected: &[f32], tolerance: f32) {
    assert_eq!(actual.len(), expected.len());
    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        assert!(
            (*actual - *expected).abs() <= tolerance,
            "index={index} actual={actual} expected={expected} tolerance={tolerance}"
        );
    }
}

fn simple_ik_chain() -> *mut MmdRuntimeIkChain {
    let bones = [
        MmdRuntimeFfiRigBone {
            parent_slot: -1,
            rest_position_xyz: [0.0, 0.0, 0.0],
            flags: 0,
            fixed_axis_xyz: [0.0, 0.0, 0.0],
        },
        MmdRuntimeFfiRigBone {
            parent_slot: 0,
            rest_position_xyz: [1.0, 0.0, 0.0],
            flags: 0,
            fixed_axis_xyz: [0.0, 0.0, 0.0],
        },
    ];
    let links = [MmdRuntimeFfiRigIkLink {
        bone_slot: 0,
        has_angle_limit: false,
        angle_limit_min_xyz: [0.0, 0.0, 0.0],
        angle_limit_max_xyz: [0.0, 0.0, 0.0],
    }];
    unsafe {
        mmd_runtime_ik_chain_create(
            bones.as_ptr(),
            bones.len(),
            1,
            links.as_ptr(),
            links.len(),
            4,
            0.0,
        )
    }
}

#[test]
fn append_solver_lifecycle_and_expected_output_use_xyzw_quaternion() {
    let config = MmdRuntimeFfiAppendConfig {
        ratio: 0.5,
        affect_rotation: true,
        affect_translation: true,
    };
    let solver = unsafe { mmd_runtime_append_solver_create(&config) };
    assert!(!solver.is_null());

    let source_position = [2.0, 4.0, -6.0];
    let source_rotation = glam::Quat::from_rotation_z(std::f32::consts::FRAC_PI_2).to_array();
    let mut out_position = [0.0; 3];
    let mut out_rotation = [0.0; 4];
    assert!(unsafe {
        mmd_runtime_append_solver_solve(
            solver,
            source_position.as_ptr(),
            source_rotation.as_ptr(),
            out_position.as_mut_ptr(),
            out_rotation.as_mut_ptr(),
        )
    });

    assert_slice_near(&out_position, &[1.0, 2.0, -3.0], 1.0e-5);
    let solved = glam::Quat::from_xyzw(
        out_rotation[0],
        out_rotation[1],
        out_rotation[2],
        out_rotation[3],
    );
    let rotated_x = solved.mul_vec3(glam::Vec3::X);
    assert_near(rotated_x.x, std::f32::consts::FRAC_1_SQRT_2, 1.0e-5);
    assert_near(rotated_x.y, std::f32::consts::FRAC_1_SQRT_2, 1.0e-5);
    assert_near(out_rotation[3], solved.w, 0.0);

    unsafe { mmd_runtime_append_solver_free(solver) };
}

#[test]
fn append_solver_rejects_null_inputs() {
    let config = MmdRuntimeFfiAppendConfig {
        ratio: 1.0,
        affect_rotation: true,
        affect_translation: true,
    };
    assert!(unsafe { mmd_runtime_append_solver_create(ptr::null()) }.is_null());
    let solver = unsafe { mmd_runtime_append_solver_create(&config) };
    assert!(!solver.is_null());

    let source_position = [0.0; 3];
    let source_rotation = [0.0, 0.0, 0.0, 1.0];
    let mut out_position = [0.0; 3];
    let mut out_rotation = [0.0; 4];
    assert!(!unsafe {
        mmd_runtime_append_solver_solve(
            ptr::null(),
            source_position.as_ptr(),
            source_rotation.as_ptr(),
            out_position.as_mut_ptr(),
            out_rotation.as_mut_ptr(),
        )
    });
    assert!(!unsafe {
        mmd_runtime_append_solver_solve(
            solver,
            ptr::null(),
            source_rotation.as_ptr(),
            out_position.as_mut_ptr(),
            out_rotation.as_mut_ptr(),
        )
    });
    assert!(!unsafe {
        mmd_runtime_append_solver_solve(
            solver,
            source_position.as_ptr(),
            ptr::null(),
            out_position.as_mut_ptr(),
            out_rotation.as_mut_ptr(),
        )
    });

    unsafe { mmd_runtime_append_solver_free(solver) };
}

#[test]
fn ik_chain_lifecycle_solve_converges_and_uses_column_major_parent_matrix() {
    let chain = simple_ik_chain();
    assert!(!chain.is_null());

    let parent_world = glam::Mat4::from_translation(glam::Vec3::new(2.0, 0.0, 0.0)).to_cols_array();
    let local_rotations = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    let goal = [2.0, 1.0, 0.0];
    let mut out_rotations = [0.0; 4];
    let mut stats = MmdRuntimeFfiIkSolveStats {
        executed_iterations: 0,
        link_steps: 0,
        final_distance: f32::MAX,
        break_reason: u32::MAX,
    };

    assert!(unsafe {
        mmd_runtime_ik_chain_solve(
            chain,
            parent_world.as_ptr(),
            ptr::null(),
            local_rotations.as_ptr(),
            goal.as_ptr(),
            1.0e-3,
            0,
            out_rotations.as_mut_ptr(),
            out_rotations.len(),
            &mut stats,
        )
    });
    assert!(
        stats.final_distance <= 1.0e-3,
        "IK should converge to the goal, stats={:?}",
        (
            stats.executed_iterations,
            stats.link_steps,
            stats.final_distance,
            stats.break_reason
        )
    );
    assert_eq!(stats.break_reason, 0);

    let solved = glam::Quat::from_xyzw(
        out_rotations[0],
        out_rotations[1],
        out_rotations[2],
        out_rotations[3],
    );
    let rotated_x = solved.mul_vec3(glam::Vec3::X);
    assert_near(rotated_x.x, 0.0, 1.0e-3);
    assert_near(rotated_x.y, 1.0, 1.0e-3);
    assert_near(out_rotations[3], solved.w, 0.0);

    unsafe { mmd_runtime_ik_chain_free(chain) };
}

#[test]
fn ik_chain_rejects_null_and_short_buffer_inputs() {
    let chain = simple_ik_chain();
    assert!(!chain.is_null());

    let local_rotations = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    let goal = [0.0, 1.0, 0.0];
    let mut out_rotations = [0.0; 4];

    assert!(
        unsafe { mmd_runtime_ik_chain_create(ptr::null(), 2, 1, ptr::null(), 1, 1, 0.0) }.is_null()
    );
    assert!(!unsafe {
        mmd_runtime_ik_chain_solve(
            ptr::null_mut(),
            ptr::null(),
            ptr::null(),
            local_rotations.as_ptr(),
            goal.as_ptr(),
            1.0e-3,
            0,
            out_rotations.as_mut_ptr(),
            out_rotations.len(),
            ptr::null_mut(),
        )
    });
    assert!(!unsafe {
        mmd_runtime_ik_chain_solve(
            chain,
            ptr::null(),
            ptr::null(),
            ptr::null(),
            goal.as_ptr(),
            1.0e-3,
            0,
            out_rotations.as_mut_ptr(),
            out_rotations.len(),
            ptr::null_mut(),
        )
    });
    assert!(!unsafe {
        mmd_runtime_ik_chain_solve(
            chain,
            ptr::null(),
            ptr::null(),
            local_rotations.as_ptr(),
            goal.as_ptr(),
            1.0e-3,
            0,
            out_rotations.as_mut_ptr(),
            out_rotations.len() - 1,
            ptr::null_mut(),
        )
    });

    unsafe { mmd_runtime_ik_chain_free(chain) };
}

#[test]
fn exports_pmx_from_parts_through_c_abi() {
    let metadata = serde_json::json!({
        "name": "ffi-parts-model",
        "englishName": "ffi-parts-model-en",
        "comment": "built through C ABI",
        "encoding": "utf-8",
        "indexSizes": {
            "vertex": 1,
            "texture": 1,
            "material": 1,
            "bone": 1,
            "morph": 1,
            "rigidBody": 1
        },
        "materialName": "ffi-default-mat"
    })
    .to_string();
    let positions = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    let normals = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0];
    let uvs = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0];
    let indices = [0, 1, 2];

    let buffer = unsafe {
        mmd_runtime_export_pmx_from_parts(
            metadata.as_ptr(),
            metadata.len(),
            positions.as_ptr(),
            3,
            normals.as_ptr(),
            uvs.as_ptr(),
            indices.as_ptr(),
            indices.len(),
            ptr::null(),
            ptr::null(),
            ptr::null(),
        )
    };
    assert!(!buffer.data.is_null());
    assert!(buffer.len > 0);

    let bytes = unsafe { slice::from_raw_parts(buffer.data, buffer.len) };
    let parsed = mmd_anim_format::parse_pmx_model(bytes).unwrap();
    assert_eq!(parsed.metadata.name, "ffi-parts-model");
    assert_eq!(parsed.metadata.english_name, "ffi-parts-model-en");
    assert_eq!(parsed.metadata.counts.vertices, 3);
    assert_eq!(parsed.metadata.counts.faces, 1);
    assert_eq!(parsed.metadata.counts.materials, 1);
    assert_eq!(parsed.metadata.counts.bones, 1);
    assert_eq!(parsed.materials[0].name, "ffi-default-mat");
    assert_eq!(parsed.geometry.indices, vec![0, 1, 2]);

    unsafe {
        mmd_runtime_byte_buffer_free(buffer);
    }
}

#[test]
fn export_pmx_from_parts_rejects_invalid_c_abi_input() {
    let metadata = "{}";
    let positions = [0.0, 0.0, 0.0];
    let normals = [0.0, 0.0, 1.0];
    let uvs = [0.0, 0.0];
    let skin_indices = [0, 0, 0, 0];

    let partial_skin = unsafe {
        mmd_runtime_export_pmx_from_parts(
            metadata.as_ptr(),
            metadata.len(),
            positions.as_ptr(),
            1,
            normals.as_ptr(),
            uvs.as_ptr(),
            ptr::null(),
            0,
            skin_indices.as_ptr(),
            ptr::null(),
            ptr::null(),
        )
    };
    assert!(partial_skin.data.is_null());
    assert_eq!(partial_skin.len, 0);
    assert_eq!(
        last_error_cstr().unwrap().to_bytes(),
        FFI_ERR_INVALID_INPUT.as_bytes()
    );

    let null_metadata = unsafe {
        mmd_runtime_export_pmx_from_parts(
            ptr::null(),
            0,
            positions.as_ptr(),
            1,
            normals.as_ptr(),
            uvs.as_ptr(),
            ptr::null(),
            0,
            ptr::null(),
            ptr::null(),
            ptr::null(),
        )
    };
    assert!(null_metadata.data.is_null());
    assert_eq!(null_metadata.len, 0);
    assert_eq!(
        last_error_cstr().unwrap().to_bytes(),
        FFI_ERR_INVALID_INPUT.as_bytes()
    );
}

#[test]
fn evaluates_rest_pose_through_c_abi() {
    let parents = [-1, 0];
    let rest_positions = [1.0, 0.0, 0.0, 0.0, 2.0, 0.0];
    let model = unsafe { mmd_runtime_model_create(parents.as_ptr(), rest_positions.as_ptr(), 2) };
    assert!(!model.is_null());
    let instance = unsafe { mmd_runtime_instance_create(model, 0) };
    assert!(!instance.is_null());

    assert!(unsafe { mmd_runtime_instance_evaluate_rest_pose(instance) });
    assert_eq!(
        unsafe { mmd_runtime_instance_world_matrix_f32_len(instance) },
        32
    );

    let mut matrices = [0.0f32; 32];
    assert!(unsafe {
        mmd_runtime_instance_copy_world_matrices(instance, matrices.as_mut_ptr(), matrices.len())
    });
    assert_eq!(matrices[12], 1.0);
    assert_eq!(matrices[16 + 12], 1.0);
    assert_eq!(matrices[16 + 13], 2.0);

    let mut skinning_matrices = [0.0f32; 32];
    assert_eq!(
        unsafe { mmd_runtime_instance_skinning_matrix_f32_len(instance) },
        32
    );
    assert!(unsafe {
        mmd_runtime_instance_copy_skinning_matrices(
            instance,
            skinning_matrices.as_mut_ptr(),
            skinning_matrices.len(),
        )
    });
    assert_eq!(skinning_matrices, matrices);

    unsafe {
        mmd_runtime_instance_free(instance);
        mmd_runtime_model_free(model);
    }
}

#[test]
fn applies_inverse_bind_through_c_abi() {
    let parents = [-1];
    let rest_positions = [2.0, 0.0, 0.0];
    let inverse_bind =
        glam::Mat4::from_translation(glam::Vec3::new(-2.0, 0.0, 0.0)).to_cols_array();
    let model = unsafe {
        mmd_runtime_model_create_with_inverse_bind(
            parents.as_ptr(),
            rest_positions.as_ptr(),
            inverse_bind.as_ptr(),
            1,
        )
    };
    assert!(!model.is_null());
    let instance = unsafe { mmd_runtime_instance_create(model, 0) };
    assert!(!instance.is_null());

    assert!(unsafe { mmd_runtime_instance_evaluate_rest_pose(instance) });

    let mut world_matrices = [0.0f32; 16];
    assert!(unsafe {
        mmd_runtime_instance_copy_world_matrices(
            instance,
            world_matrices.as_mut_ptr(),
            world_matrices.len(),
        )
    });
    assert_eq!(world_matrices[12], 2.0);

    let mut skinning_matrices = [0.0f32; 16];
    assert!(unsafe {
        mmd_runtime_instance_copy_skinning_matrices(
            instance,
            skinning_matrices.as_mut_ptr(),
            skinning_matrices.len(),
        )
    });
    assert_eq!(skinning_matrices[12], 0.0);

    unsafe {
        mmd_runtime_instance_free(instance);
        mmd_runtime_model_free(model);
    }
}

#[test]
fn creates_ik_solver_through_full_c_abi() {
    let parents = [-1, 0, 1];
    let rest_positions = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    let ik_links = [MmdRuntimeFfiIkLink {
        bone_index: 1,
        flags: IK_LINK_FLAG_ANGLE_LIMIT,
        angle_limit_min_xyz: [-1.0, -0.5, -0.25],
        angle_limit_max_xyz: [1.0, 0.5, 0.25],
    }];
    let ik_solvers = [MmdRuntimeFfiIkSolver {
        ik_bone_index: 0,
        target_bone_index: 2,
        link_offset: 0,
        link_count: 1,
        iteration_count: 2,
        limit_angle: 0.5,
    }];
    let model = unsafe {
        mmd_runtime_model_create_full(
            parents.as_ptr(),
            rest_positions.as_ptr(),
            ptr::null(),
            3,
            ik_solvers.as_ptr(),
            ik_solvers.len(),
            ik_links.as_ptr(),
            ik_links.len(),
            ptr::null(),
            0,
        )
    };
    assert!(!model.is_null());
    let instance = unsafe { mmd_runtime_instance_create(model, 0) };
    assert!(!instance.is_null());

    assert_eq!(unsafe { mmd_runtime_instance_ik_enabled_len(instance) }, 1);
    let mut ik_enabled = [0u8; 1];
    assert!(unsafe {
        mmd_runtime_instance_copy_ik_enabled(instance, ik_enabled.as_mut_ptr(), ik_enabled.len())
    });
    assert_eq!(ik_enabled[0], 1);

    unsafe {
        mmd_runtime_instance_free(instance);
        mmd_runtime_model_free(model);
    }
}

#[test]
fn evaluates_clip_frame_through_c_abi() {
    let parents = [-1];
    let rest_positions = [0.0, 0.0, 0.0];
    let model = unsafe { mmd_runtime_model_create(parents.as_ptr(), rest_positions.as_ptr(), 1) };
    assert!(!model.is_null());
    let instance = unsafe { mmd_runtime_instance_create_with_counts(model, 1, 1) };
    assert!(!instance.is_null());

    let bone_tracks = [MmdRuntimeFfiBoneTrack {
        bone_index: 0,
        keyframe_offset: 0,
        keyframe_count: 2,
    }];
    let bone_keyframes = [
        MmdRuntimeFfiBoneKeyframe {
            frame: 0,
            position_xyz: [0.0, 0.0, 0.0],
            rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
        },
        MmdRuntimeFfiBoneKeyframe {
            frame: 60,
            position_xyz: [2.0, 0.0, 0.0],
            rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
        },
    ];
    let morph_tracks = [MmdRuntimeFfiMorphTrack {
        morph_index: 0,
        keyframe_offset: 0,
        keyframe_count: 2,
    }];
    let morph_keyframes = [
        MmdRuntimeFfiMorphKeyframe {
            frame: 0,
            weight: 0.0,
        },
        MmdRuntimeFfiMorphKeyframe {
            frame: 60,
            weight: 1.0,
        },
    ];
    let property_keyframes = [
        MmdRuntimeFfiPropertyKeyframe {
            frame: 0,
            ik_enabled_offset: 0,
            ik_enabled_count: 1,
        },
        MmdRuntimeFfiPropertyKeyframe {
            frame: 30,
            ik_enabled_offset: 1,
            ik_enabled_count: 1,
        },
    ];
    let property_ik_enabled = [1u8, 0u8];
    let clip = unsafe {
        mmd_runtime_clip_create(
            bone_tracks.as_ptr(),
            bone_tracks.len(),
            bone_keyframes.as_ptr(),
            bone_keyframes.len(),
            morph_tracks.as_ptr(),
            morph_tracks.len(),
            morph_keyframes.as_ptr(),
            morph_keyframes.len(),
            property_keyframes.as_ptr(),
            property_keyframes.len(),
            property_ik_enabled.as_ptr(),
            property_ik_enabled.len(),
        )
    };
    assert!(!clip.is_null());

    assert!(unsafe { mmd_runtime_instance_evaluate_clip_frame(instance, clip, 30.0) });

    let mut matrices = [0.0f32; 16];
    assert!(unsafe {
        mmd_runtime_instance_copy_world_matrices(instance, matrices.as_mut_ptr(), matrices.len())
    });
    assert_eq!(matrices[12], 1.0);

    let mut morph_weights = [0.0f32; 1];
    assert_eq!(
        unsafe { mmd_runtime_instance_morph_weight_len(instance) },
        1
    );
    assert!(unsafe {
        mmd_runtime_instance_copy_morph_weights(
            instance,
            morph_weights.as_mut_ptr(),
            morph_weights.len(),
        )
    });
    assert_eq!(morph_weights[0], 0.5);

    let mut ik_enabled = [1u8; 1];
    assert_eq!(unsafe { mmd_runtime_instance_ik_enabled_len(instance) }, 1);
    assert!(unsafe {
        mmd_runtime_instance_copy_ik_enabled(instance, ik_enabled.as_mut_ptr(), ik_enabled.len())
    });
    assert_eq!(ik_enabled[0], 0);

    unsafe {
        mmd_runtime_clip_free(clip);
        mmd_runtime_instance_free(instance);
        mmd_runtime_model_free(model);
    }
}

#[test]
fn evaluates_clip_frame_batch_through_c_abi_without_mutating_source_instance() {
    let parents = [-1];
    let rest_positions = [1.0, 0.0, 0.0];
    let model = unsafe { mmd_runtime_model_create(parents.as_ptr(), rest_positions.as_ptr(), 1) };
    assert!(!model.is_null());
    let instance = unsafe { mmd_runtime_instance_create_with_counts(model, 1, 1) };
    assert!(!instance.is_null());

    let bone_tracks = [MmdRuntimeFfiBoneTrack {
        bone_index: 0,
        keyframe_offset: 0,
        keyframe_count: 2,
    }];
    let bone_keyframes = [
        MmdRuntimeFfiBoneKeyframe {
            frame: 0,
            position_xyz: [0.0, 0.0, 0.0],
            rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
        },
        MmdRuntimeFfiBoneKeyframe {
            frame: 60,
            position_xyz: [2.0, 0.0, 0.0],
            rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
        },
    ];
    let morph_tracks = [MmdRuntimeFfiMorphTrack {
        morph_index: 0,
        keyframe_offset: 0,
        keyframe_count: 2,
    }];
    let morph_keyframes = [
        MmdRuntimeFfiMorphKeyframe {
            frame: 0,
            weight: 0.0,
        },
        MmdRuntimeFfiMorphKeyframe {
            frame: 60,
            weight: 1.0,
        },
    ];
    let property_keyframes = [
        MmdRuntimeFfiPropertyKeyframe {
            frame: 0,
            ik_enabled_offset: 0,
            ik_enabled_count: 1,
        },
        MmdRuntimeFfiPropertyKeyframe {
            frame: 30,
            ik_enabled_offset: 1,
            ik_enabled_count: 1,
        },
    ];
    let property_ik_enabled = [1u8, 0u8];
    let clip = unsafe {
        mmd_runtime_clip_create(
            bone_tracks.as_ptr(),
            bone_tracks.len(),
            bone_keyframes.as_ptr(),
            bone_keyframes.len(),
            morph_tracks.as_ptr(),
            morph_tracks.len(),
            morph_keyframes.as_ptr(),
            morph_keyframes.len(),
            property_keyframes.as_ptr(),
            property_keyframes.len(),
            property_ik_enabled.as_ptr(),
            property_ik_enabled.len(),
        )
    };
    assert!(!clip.is_null());

    assert!(unsafe { mmd_runtime_instance_evaluate_clip_frame(instance, clip, 30.0) });
    let mut source_morph = [0.0f32; 1];
    assert!(unsafe {
        mmd_runtime_instance_copy_morph_weights(instance, source_morph.as_mut_ptr(), 1)
    });
    assert_eq!(source_morph[0], 0.5);

    assert_eq!(
        unsafe { mmd_runtime_instance_clip_frame_batch_world_matrix_f32_len(instance, 3) },
        48
    );
    assert_eq!(
        unsafe { mmd_runtime_instance_clip_frame_batch_morph_weight_f32_len(instance, 3) },
        3
    );

    let mut batch_world = [0.0f32; 48];
    let mut batch_morphs = [0.0f32; 3];
    assert!(unsafe {
        mmd_runtime_instance_evaluate_clip_frame_batch(
            instance,
            clip,
            0.0,
            30.0,
            3,
            2,
            batch_world.as_mut_ptr(),
            batch_world.len(),
            batch_morphs.as_mut_ptr(),
            batch_morphs.len(),
        )
    });

    assert_eq!(batch_world[12], 1.0);
    assert_eq!(batch_world[16 + 12], 2.0);
    assert_eq!(batch_world[32 + 12], 3.0);
    assert_slice_near(&batch_morphs, &[0.0, 0.5, 1.0], 0.0);

    let mut auto_worker_world = [0.0f32; 48];
    let mut auto_worker_morphs = [0.0f32; 3];
    assert!(unsafe {
        mmd_runtime_instance_evaluate_clip_frame_batch(
            instance,
            clip,
            0.0,
            30.0,
            3,
            0,
            auto_worker_world.as_mut_ptr(),
            auto_worker_world.len(),
            auto_worker_morphs.as_mut_ptr(),
            auto_worker_morphs.len(),
        )
    });
    assert_slice_near(&auto_worker_world, &batch_world, 0.0);
    assert_slice_near(&auto_worker_morphs, &batch_morphs, 0.0);

    assert!(unsafe {
        mmd_runtime_instance_evaluate_clip_frame_batch(
            instance,
            clip,
            0.0,
            30.0,
            0,
            0,
            ptr::null_mut(),
            0,
            ptr::null_mut(),
            0,
        )
    });

    let mut source_morph_after = [0.0f32; 1];
    assert!(unsafe {
        mmd_runtime_instance_copy_morph_weights(instance, source_morph_after.as_mut_ptr(), 1)
    });
    assert_eq!(source_morph_after[0], 0.5);

    assert!(!unsafe {
        mmd_runtime_instance_evaluate_clip_frame_batch(
            instance,
            clip,
            0.0,
            30.0,
            3,
            2,
            batch_world.as_mut_ptr(),
            batch_world.len() - 1,
            batch_morphs.as_mut_ptr(),
            batch_morphs.len(),
        )
    });
    assert!(!unsafe {
        mmd_runtime_instance_evaluate_clip_frame_batch(
            instance,
            clip,
            f32::NAN,
            30.0,
            3,
            2,
            batch_world.as_mut_ptr(),
            batch_world.len(),
            batch_morphs.as_mut_ptr(),
            batch_morphs.len(),
        )
    });

    unsafe {
        mmd_runtime_clip_free(clip);
        mmd_runtime_instance_free(instance);
        mmd_runtime_model_free(model);
    }
}

#[test]
fn evaluates_clip_frame_batch_allows_null_morph_buffer_when_model_has_no_morphs() {
    let parents = [-1];
    let rest_positions = [1.0, 0.0, 0.0];
    let model = unsafe { mmd_runtime_model_create(parents.as_ptr(), rest_positions.as_ptr(), 1) };
    assert!(!model.is_null());
    let instance = unsafe { mmd_runtime_instance_create(model, 0) };
    assert!(!instance.is_null());

    let bone_tracks = [MmdRuntimeFfiBoneTrack {
        bone_index: 0,
        keyframe_offset: 0,
        keyframe_count: 2,
    }];
    let bone_keyframes = [
        MmdRuntimeFfiBoneKeyframe {
            frame: 0,
            position_xyz: [0.0, 0.0, 0.0],
            rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
        },
        MmdRuntimeFfiBoneKeyframe {
            frame: 10,
            position_xyz: [1.0, 0.0, 0.0],
            rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
        },
    ];
    let clip = unsafe {
        mmd_runtime_clip_create(
            bone_tracks.as_ptr(),
            bone_tracks.len(),
            bone_keyframes.as_ptr(),
            bone_keyframes.len(),
            ptr::null(),
            0,
            ptr::null(),
            0,
            ptr::null(),
            0,
            ptr::null(),
            0,
        )
    };
    assert!(!clip.is_null());

    assert_eq!(
        unsafe { mmd_runtime_instance_clip_frame_batch_morph_weight_f32_len(instance, 2) },
        0
    );
    let mut batch_world = [0.0f32; 32];
    assert!(unsafe {
        mmd_runtime_instance_evaluate_clip_frame_batch(
            instance,
            clip,
            0.0,
            10.0,
            2,
            2,
            batch_world.as_mut_ptr(),
            batch_world.len(),
            ptr::null_mut(),
            0,
        )
    });
    assert_eq!(batch_world[12], 1.0);
    assert_eq!(batch_world[16 + 12], 2.0);

    unsafe {
        mmd_runtime_clip_free(clip);
        mmd_runtime_instance_free(instance);
        mmd_runtime_model_free(model);
    }
}

#[test]
fn evaluates_clip_frame_without_ik_through_c_abi() {
    let parents = [-1];
    let rest_positions = [0.0, 0.0, 0.0];
    let model = unsafe { mmd_runtime_model_create(parents.as_ptr(), rest_positions.as_ptr(), 1) };
    assert!(!model.is_null());
    let instance = unsafe { mmd_runtime_instance_create(model, 0) };
    assert!(!instance.is_null());

    let bone_tracks = [MmdRuntimeFfiBoneTrack {
        bone_index: 0,
        keyframe_offset: 0,
        keyframe_count: 2,
    }];
    let bone_keyframes = [
        MmdRuntimeFfiBoneKeyframe {
            frame: 0,
            position_xyz: [0.0, 0.0, 0.0],
            rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
        },
        MmdRuntimeFfiBoneKeyframe {
            frame: 60,
            position_xyz: [2.0, 0.0, 0.0],
            rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
        },
    ];
    let clip = unsafe {
        mmd_runtime_clip_create(
            bone_tracks.as_ptr(),
            bone_tracks.len(),
            bone_keyframes.as_ptr(),
            bone_keyframes.len(),
            ptr::null(),
            0,
            ptr::null(),
            0,
            ptr::null(),
            0,
            ptr::null(),
            0,
        )
    };
    assert!(!clip.is_null());

    assert!(unsafe { mmd_runtime_instance_evaluate_clip_frame_without_ik(instance, clip, 30.0) });
    let mut matrices = [0.0f32; 16];
    assert!(unsafe {
        mmd_runtime_instance_copy_world_matrices(instance, matrices.as_mut_ptr(), matrices.len())
    });
    assert_eq!(matrices[12], 1.0);

    unsafe {
        mmd_runtime_clip_free(clip);
        mmd_runtime_instance_free(instance);
        mmd_runtime_model_free(model);
    }
}

#[test]
fn evaluates_append_rotation_through_c_abi() {
    let parents = [-1, -1, 1];
    let rest_positions = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    let append = [MmdRuntimeFfiAppendTransform {
        target_bone_index: 1,
        source_bone_index: 0,
        ratio: 1.0,
        flags: APPEND_FLAG_ROTATION,
    }];
    let model = unsafe {
        mmd_runtime_model_create_with_append(
            parents.as_ptr(),
            rest_positions.as_ptr(),
            3,
            append.as_ptr(),
            append.len(),
        )
    };
    assert!(!model.is_null());
    let instance = unsafe { mmd_runtime_instance_create(model, 0) };
    assert!(!instance.is_null());

    let bone_tracks = [MmdRuntimeFfiBoneTrack {
        bone_index: 0,
        keyframe_offset: 0,
        keyframe_count: 1,
    }];
    let rotation = glam::Quat::from_rotation_z(std::f32::consts::FRAC_PI_2).to_array();
    let bone_keyframes = [MmdRuntimeFfiBoneKeyframe {
        frame: 0,
        position_xyz: [0.0, 0.0, 0.0],
        rotation_xyzw: rotation,
    }];
    let clip = unsafe {
        mmd_runtime_clip_create(
            bone_tracks.as_ptr(),
            bone_tracks.len(),
            bone_keyframes.as_ptr(),
            bone_keyframes.len(),
            ptr::null(),
            0,
            ptr::null(),
            0,
            ptr::null(),
            0,
            ptr::null(),
            0,
        )
    };
    assert!(!clip.is_null());

    assert!(unsafe { mmd_runtime_instance_evaluate_clip_frame(instance, clip, 0.0) });
    let mut matrices = [0.0f32; 48];
    assert!(unsafe {
        mmd_runtime_instance_copy_world_matrices(instance, matrices.as_mut_ptr(), matrices.len())
    });
    assert!(matrices[32 + 12].abs() < 1.0e-5);
    assert!((matrices[32 + 13] - 1.0).abs() < 1.0e-5);

    unsafe {
        mmd_runtime_clip_free(clip);
        mmd_runtime_instance_free(instance);
        mmd_runtime_model_free(model);
    }
}

#[test]
fn copy_functions_reject_short_buffer() {
    let parents = [-1, 0];
    let rest_positions = [1.0, 0.0, 0.0, 0.0, 2.0, 0.0];
    let model = unsafe { mmd_runtime_model_create(parents.as_ptr(), rest_positions.as_ptr(), 2) };
    assert!(!model.is_null());
    let instance = unsafe { mmd_runtime_instance_create_with_counts(model, 1, 1) };
    assert!(!instance.is_null());

    assert!(unsafe { mmd_runtime_instance_evaluate_rest_pose(instance) });

    let mut buf32 = [0.0f32; 32];
    assert!(!unsafe { mmd_runtime_instance_copy_world_matrices(instance, buf32.as_mut_ptr(), 31) });
    assert!(!unsafe { mmd_runtime_instance_copy_world_matrices(instance, buf32.as_mut_ptr(), 0) });

    assert!(!unsafe {
        mmd_runtime_instance_copy_skinning_matrices(instance, buf32.as_mut_ptr(), 31)
    });
    assert!(!unsafe {
        mmd_runtime_instance_copy_skinning_matrices(instance, buf32.as_mut_ptr(), 0)
    });

    let mut buf_f32 = [0.0f32; 1];
    assert!(!unsafe { mmd_runtime_instance_copy_morph_weights(instance, buf_f32.as_mut_ptr(), 0) });

    let mut buf_u8 = [0u8; 1];
    assert!(!unsafe { mmd_runtime_instance_copy_ik_enabled(instance, buf_u8.as_mut_ptr(), 0) });

    unsafe {
        mmd_runtime_instance_free(instance);
        mmd_runtime_model_free(model);
    }
}

#[test]
fn applies_transform_order_to_append_chain_through_c_abi() {
    let parents = [-1, -1, -1, 1];
    let rest_positions = [
        0.0, 0.0, 0.0, //
        0.0, 0.0, 0.0, //
        0.0, 0.0, 0.0, //
        1.0, 0.0, 0.0,
    ];
    let transform_orders = [0, 2, 1, 3];
    let append = [
        MmdRuntimeFfiAppendTransform {
            target_bone_index: 2,
            source_bone_index: 0,
            ratio: 1.0,
            flags: APPEND_FLAG_ROTATION,
        },
        MmdRuntimeFfiAppendTransform {
            target_bone_index: 1,
            source_bone_index: 2,
            ratio: 1.0,
            flags: APPEND_FLAG_ROTATION,
        },
    ];
    let model = unsafe {
        mmd_runtime_model_create_full_with_transform_order(
            parents.as_ptr(),
            rest_positions.as_ptr(),
            ptr::null(),
            transform_orders.as_ptr(),
            4,
            ptr::null(),
            0,
            ptr::null(),
            0,
            append.as_ptr(),
            append.len(),
        )
    };
    assert!(!model.is_null());
    let instance = unsafe { mmd_runtime_instance_create(model, 0) };
    assert!(!instance.is_null());

    let bone_tracks = [MmdRuntimeFfiBoneTrack {
        bone_index: 0,
        keyframe_offset: 0,
        keyframe_count: 1,
    }];
    let rotation = glam::Quat::from_rotation_z(std::f32::consts::FRAC_PI_2).to_array();
    let bone_keyframes = [MmdRuntimeFfiBoneKeyframe {
        frame: 0,
        position_xyz: [0.0, 0.0, 0.0],
        rotation_xyzw: rotation,
    }];
    let clip = unsafe {
        mmd_runtime_clip_create(
            bone_tracks.as_ptr(),
            bone_tracks.len(),
            bone_keyframes.as_ptr(),
            bone_keyframes.len(),
            ptr::null(),
            0,
            ptr::null(),
            0,
            ptr::null(),
            0,
            ptr::null(),
            0,
        )
    };
    assert!(!clip.is_null());

    assert!(unsafe { mmd_runtime_instance_evaluate_clip_frame(instance, clip, 0.0) });
    let mut matrices = [0.0f32; 64];
    assert!(unsafe {
        mmd_runtime_instance_copy_world_matrices(instance, matrices.as_mut_ptr(), matrices.len())
    });
    assert!(matrices[48 + 12].abs() < 1.0e-5);
    assert!((matrices[48 + 13] - 1.0).abs() < 1.0e-5);

    unsafe {
        mmd_runtime_clip_free(clip);
        mmd_runtime_instance_free(instance);
        mmd_runtime_model_free(model);
    }
}

#[test]
fn creates_bone_morph_through_c_abi() {
    let parents = [-1];
    let rest_positions = [0.0, 0.0, 0.0];
    let bone_morphs = [MmdRuntimeFfiBoneMorphOffset {
        morph_index: 0,
        target_bone_index: 0,
        position_offset_xyz: [2.0, 0.0, 0.0],
        rotation_offset_xyzw: [0.0, 0.0, 0.0, 1.0],
    }];
    let model = unsafe {
        mmd_runtime_model_create_full_with_morphs(
            parents.as_ptr(),
            rest_positions.as_ptr(),
            ptr::null(),
            ptr::null(),
            1,
            ptr::null(),
            0,
            ptr::null(),
            0,
            ptr::null(),
            0,
            1,
            bone_morphs.as_ptr(),
            bone_morphs.len(),
            ptr::null(),
            0,
        )
    };
    assert!(!model.is_null());
    let instance = unsafe { mmd_runtime_instance_create(model, 1) };
    assert!(!instance.is_null());

    let bone_tracks = [MmdRuntimeFfiBoneTrack {
        bone_index: 0,
        keyframe_offset: 0,
        keyframe_count: 1,
    }];
    let rotation = glam::Quat::from_rotation_z(std::f32::consts::FRAC_PI_2).to_array();
    let bone_keyframes = [MmdRuntimeFfiBoneKeyframe {
        frame: 0,
        position_xyz: [0.0, 0.0, 0.0],
        rotation_xyzw: rotation,
    }];
    let morph_tracks = [MmdRuntimeFfiMorphTrack {
        morph_index: 0,
        keyframe_offset: 0,
        keyframe_count: 2,
    }];
    let morph_keyframes = [
        MmdRuntimeFfiMorphKeyframe {
            frame: 0,
            weight: 0.0,
        },
        MmdRuntimeFfiMorphKeyframe {
            frame: 60,
            weight: 1.0,
        },
    ];
    let clip = unsafe {
        mmd_runtime_clip_create(
            bone_tracks.as_ptr(),
            bone_tracks.len(),
            bone_keyframes.as_ptr(),
            bone_keyframes.len(),
            morph_tracks.as_ptr(),
            morph_tracks.len(),
            morph_keyframes.as_ptr(),
            morph_keyframes.len(),
            ptr::null(),
            0,
            ptr::null(),
            0,
        )
    };
    assert!(!clip.is_null());

    assert!(unsafe { mmd_runtime_instance_evaluate_clip_frame(instance, clip, 60.0) });
    let mut morph_weights = [0.0f32; 1];
    assert!(unsafe {
        mmd_runtime_instance_copy_morph_weights(
            instance,
            morph_weights.as_mut_ptr(),
            morph_weights.len(),
        )
    });
    assert_eq!(morph_weights[0], 1.0);

    let mut matrices = [0.0f32; 16];
    assert!(unsafe {
        mmd_runtime_instance_copy_world_matrices(instance, matrices.as_mut_ptr(), matrices.len())
    });
    assert!((matrices[12] - 2.0).abs() < 1.0e-5);

    unsafe {
        mmd_runtime_clip_free(clip);
        mmd_runtime_instance_free(instance);
        mmd_runtime_model_free(model);
    }
}

#[test]
fn rejects_null_bone_morph_with_nonzero_count() {
    let parents = [-1];
    let rest_positions = [0.0, 0.0, 0.0];
    let model = unsafe {
        mmd_runtime_model_create_full_with_morphs(
            parents.as_ptr(),
            rest_positions.as_ptr(),
            ptr::null(),
            ptr::null(),
            1,
            ptr::null(),
            0,
            ptr::null(),
            0,
            ptr::null(),
            0,
            1,
            ptr::null(),
            1,
            ptr::null(),
            0,
        )
    };
    assert!(model.is_null());
}

#[test]
fn rejects_morph_count_zero_with_bone_data() {
    let parents = [-1];
    let rest_positions = [0.0, 0.0, 0.0];
    let bone_morphs = [MmdRuntimeFfiBoneMorphOffset {
        morph_index: 0,
        target_bone_index: 0,
        position_offset_xyz: [1.0, 0.0, 0.0],
        rotation_offset_xyzw: [0.0, 0.0, 0.0, 1.0],
    }];
    let model = unsafe {
        mmd_runtime_model_create_full_with_morphs(
            parents.as_ptr(),
            rest_positions.as_ptr(),
            ptr::null(),
            ptr::null(),
            1,
            ptr::null(),
            0,
            ptr::null(),
            0,
            ptr::null(),
            0,
            0,
            bone_morphs.as_ptr(),
            bone_morphs.len(),
            ptr::null(),
            0,
        )
    };
    assert!(model.is_null());
}

// -----------------------------------------------------------------------
// Phase 6: direct output view tests
// -----------------------------------------------------------------------

#[test]
fn bone_count_returns_correct_value() {
    let parents = [-1, 0, 1];
    let rest_positions = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 2.0, 0.0, 0.0];
    let model = unsafe { mmd_runtime_model_create(parents.as_ptr(), rest_positions.as_ptr(), 3) };
    assert!(!model.is_null());
    let instance = unsafe { mmd_runtime_instance_create(model, 0) };
    assert!(!instance.is_null());

    assert_eq!(unsafe { mmd_runtime_instance_bone_count(instance) }, 3);

    unsafe {
        mmd_runtime_instance_free(instance);
        mmd_runtime_model_free(model);
    }
}

#[test]
fn model_count_accessors_return_expected_values() {
    let parents = [-1, 0, 1];
    let rest_positions = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 2.0, 0.0, 0.0];
    let transform_orders = [0, 1, 2];
    let ik_links = [MmdRuntimeFfiIkLink {
        bone_index: 1,
        flags: 0,
        angle_limit_min_xyz: [0.0, 0.0, 0.0],
        angle_limit_max_xyz: [0.0, 0.0, 0.0],
    }];
    let ik_solvers = [MmdRuntimeFfiIkSolver {
        ik_bone_index: 2,
        target_bone_index: 0,
        link_offset: 0,
        link_count: 1,
        iteration_count: 1,
        limit_angle: 1.0,
    }];
    let bone_morphs = [MmdRuntimeFfiBoneMorphOffset {
        morph_index: 1,
        target_bone_index: 0,
        position_offset_xyz: [1.0, 0.0, 0.0],
        rotation_offset_xyzw: [0.0, 0.0, 0.0, 1.0],
    }];
    let model = unsafe {
        mmd_runtime_model_create_full_with_morphs(
            parents.as_ptr(),
            rest_positions.as_ptr(),
            ptr::null(),
            transform_orders.as_ptr(),
            3,
            ik_solvers.as_ptr(),
            ik_solvers.len(),
            ik_links.as_ptr(),
            ik_links.len(),
            ptr::null(),
            0,
            2,
            bone_morphs.as_ptr(),
            bone_morphs.len(),
            ptr::null(),
            0,
        )
    };
    assert!(!model.is_null());

    assert_eq!(unsafe { mmd_runtime_model_bone_count(model) }, 3);
    assert_eq!(unsafe { mmd_runtime_model_morph_count(model) }, 2);
    assert_eq!(unsafe { mmd_runtime_model_ik_count(model) }, 1);

    unsafe {
        mmd_runtime_model_free(model);
    }
}

#[test]
fn model_count_accessors_return_zero_for_null() {
    assert_eq!(unsafe { mmd_runtime_model_bone_count(ptr::null()) }, 0);
    assert_eq!(unsafe { mmd_runtime_model_morph_count(ptr::null()) }, 0);
    assert_eq!(unsafe { mmd_runtime_model_ik_count(ptr::null()) }, 0);
}

#[test]
fn instance_create_for_model_uses_model_counts() {
    let parents = [-1, 0, 1];
    let rest_positions = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 2.0, 0.0, 0.0];
    let transform_orders = [0, 1, 2];
    let ik_links = [MmdRuntimeFfiIkLink {
        bone_index: 1,
        flags: 0,
        angle_limit_min_xyz: [0.0, 0.0, 0.0],
        angle_limit_max_xyz: [0.0, 0.0, 0.0],
    }];
    let ik_solvers = [MmdRuntimeFfiIkSolver {
        ik_bone_index: 2,
        target_bone_index: 0,
        link_offset: 0,
        link_count: 1,
        iteration_count: 1,
        limit_angle: 1.0,
    }];
    let bone_morphs = [MmdRuntimeFfiBoneMorphOffset {
        morph_index: 1,
        target_bone_index: 0,
        position_offset_xyz: [1.0, 0.0, 0.0],
        rotation_offset_xyzw: [0.0, 0.0, 0.0, 1.0],
    }];
    let model = unsafe {
        mmd_runtime_model_create_full_with_morphs(
            parents.as_ptr(),
            rest_positions.as_ptr(),
            ptr::null(),
            transform_orders.as_ptr(),
            3,
            ik_solvers.as_ptr(),
            ik_solvers.len(),
            ik_links.as_ptr(),
            ik_links.len(),
            ptr::null(),
            0,
            2,
            bone_morphs.as_ptr(),
            bone_morphs.len(),
            ptr::null(),
            0,
        )
    };
    assert!(!model.is_null());
    let instance = unsafe { mmd_runtime_instance_create_for_model(model) };
    assert!(!instance.is_null());

    assert_eq!(unsafe { mmd_runtime_instance_bone_count(instance) }, 3);
    assert_eq!(
        unsafe { mmd_runtime_instance_morph_weight_len(instance) },
        2
    );
    assert_eq!(unsafe { mmd_runtime_instance_ik_enabled_len(instance) }, 1);

    unsafe {
        mmd_runtime_instance_free(instance);
        mmd_runtime_model_free(model);
    }
}

#[test]
fn instance_create_for_model_returns_null_for_null() {
    assert!(unsafe { mmd_runtime_instance_create_for_model(ptr::null()) }.is_null());
}

#[test]
fn bone_count_returns_zero_for_null() {
    assert_eq!(unsafe { mmd_runtime_instance_bone_count(ptr::null()) }, 0);
}

#[test]
fn pointer_view_returns_non_null_after_evaluation() {
    let parents = [-1, 0];
    let rest_positions = [1.0, 0.0, 0.0, 0.0, 2.0, 0.0];
    let model = unsafe { mmd_runtime_model_create(parents.as_ptr(), rest_positions.as_ptr(), 2) };
    assert!(!model.is_null());
    let instance = unsafe { mmd_runtime_instance_create(model, 0) };
    assert!(!instance.is_null());

    assert!(unsafe { mmd_runtime_instance_evaluate_rest_pose(instance) });

    let world_ptr = unsafe { mmd_runtime_instance_world_matrices(instance) };
    assert!(!world_ptr.is_null());

    let skin_ptr = unsafe { mmd_runtime_instance_skinning_matrices(instance) };
    assert!(!skin_ptr.is_null());

    unsafe {
        mmd_runtime_instance_free(instance);
        mmd_runtime_model_free(model);
    }
}

#[test]
fn pointer_view_returns_null_for_null_instance() {
    assert!(unsafe { mmd_runtime_instance_world_matrices(ptr::null()) }.is_null());
    assert!(unsafe { mmd_runtime_instance_skinning_matrices(ptr::null()) }.is_null());
}

#[test]
fn pointer_view_contains_expected_translation() {
    let parents = [-1, 0];
    let rest_positions = [1.0, 0.0, 0.0, 0.0, 2.0, 0.0];
    let model = unsafe { mmd_runtime_model_create(parents.as_ptr(), rest_positions.as_ptr(), 2) };
    assert!(!model.is_null());
    let instance = unsafe { mmd_runtime_instance_create(model, 0) };
    assert!(!instance.is_null());

    assert!(unsafe { mmd_runtime_instance_evaluate_rest_pose(instance) });

    let world_ptr = unsafe { mmd_runtime_instance_world_matrices(instance) };
    assert!(!world_ptr.is_null());

    // column-major: translation is at indices [12, 13, 14]
    unsafe {
        assert_eq!(*world_ptr.add(12), 1.0);
        assert_eq!(*world_ptr.add(16 + 12), 1.0);
        assert_eq!(*world_ptr.add(16 + 13), 2.0);
    }

    unsafe {
        mmd_runtime_instance_free(instance);
        mmd_runtime_model_free(model);
    }
}

#[test]
fn pointer_view_consistent_with_copy_api() {
    let parents = [-1, 0];
    let rest_positions = [1.0, 0.0, 0.0, 0.0, 2.0, 0.0];
    let model = unsafe { mmd_runtime_model_create(parents.as_ptr(), rest_positions.as_ptr(), 2) };
    assert!(!model.is_null());
    let instance = unsafe { mmd_runtime_instance_create(model, 0) };
    assert!(!instance.is_null());

    assert!(unsafe { mmd_runtime_instance_evaluate_rest_pose(instance) });

    // Read via pointer view
    let world_ptr = unsafe { mmd_runtime_instance_world_matrices(instance) };
    let world_slice = unsafe { std::slice::from_raw_parts(world_ptr, 32) };

    // Read via copy API
    let mut copy_buf = [0.0f32; 32];
    assert!(unsafe {
        mmd_runtime_instance_copy_world_matrices(instance, copy_buf.as_mut_ptr(), copy_buf.len())
    });

    assert_eq!(world_slice, &copy_buf);

    // Same for skinning
    let skin_ptr = unsafe { mmd_runtime_instance_skinning_matrices(instance) };
    let skin_slice = unsafe { std::slice::from_raw_parts(skin_ptr, 32) };

    let mut skin_copy = [0.0f32; 32];
    assert!(unsafe {
        mmd_runtime_instance_copy_skinning_matrices(
            instance,
            skin_copy.as_mut_ptr(),
            skin_copy.len(),
        )
    });

    assert_eq!(skin_slice, &skin_copy);

    unsafe {
        mmd_runtime_instance_free(instance);
        mmd_runtime_model_free(model);
    }
}

// -----------------------------------------------------------------------
// Phase 6b: morph/IK direct pointer view tests
// -----------------------------------------------------------------------

#[test]
fn morph_ik_direct_pointer_returns_null_for_null_instance() {
    assert!(unsafe { mmd_runtime_instance_morph_weights(ptr::null()) }.is_null());
    assert!(unsafe { mmd_runtime_instance_ik_enabled(ptr::null()) }.is_null());
}

#[test]
fn morph_ik_direct_pointer_consistent_with_copy_api() {
    let parents = [-1];
    let rest_positions = [0.0, 0.0, 0.0];
    let model = unsafe { mmd_runtime_model_create(parents.as_ptr(), rest_positions.as_ptr(), 1) };
    assert!(!model.is_null());
    let instance = unsafe { mmd_runtime_instance_create_with_counts(model, 1, 1) };
    assert!(!instance.is_null());

    let bone_tracks = [MmdRuntimeFfiBoneTrack {
        bone_index: 0,
        keyframe_offset: 0,
        keyframe_count: 1,
    }];
    let bone_keyframes = [MmdRuntimeFfiBoneKeyframe {
        frame: 0,
        position_xyz: [0.0, 0.0, 0.0],
        rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
    }];
    let morph_tracks = [MmdRuntimeFfiMorphTrack {
        morph_index: 0,
        keyframe_offset: 0,
        keyframe_count: 2,
    }];
    let morph_keyframes = [
        MmdRuntimeFfiMorphKeyframe {
            frame: 0,
            weight: 0.0,
        },
        MmdRuntimeFfiMorphKeyframe {
            frame: 60,
            weight: 1.0,
        },
    ];
    let property_keyframes = [MmdRuntimeFfiPropertyKeyframe {
        frame: 0,
        ik_enabled_offset: 0,
        ik_enabled_count: 1,
    }];
    let property_ik_enabled = [1u8];
    let clip = unsafe {
        mmd_runtime_clip_create(
            bone_tracks.as_ptr(),
            bone_tracks.len(),
            bone_keyframes.as_ptr(),
            bone_keyframes.len(),
            morph_tracks.as_ptr(),
            morph_tracks.len(),
            morph_keyframes.as_ptr(),
            morph_keyframes.len(),
            property_keyframes.as_ptr(),
            property_keyframes.len(),
            property_ik_enabled.as_ptr(),
            property_ik_enabled.len(),
        )
    };
    assert!(!clip.is_null());

    assert!(unsafe { mmd_runtime_instance_evaluate_clip_frame(instance, clip, 30.0) });

    // Direct pointer read
    let morph_ptr = unsafe { mmd_runtime_instance_morph_weights(instance) };
    assert!(!morph_ptr.is_null());
    let morph_slice = unsafe { std::slice::from_raw_parts(morph_ptr, 1) };

    let ik_ptr = unsafe { mmd_runtime_instance_ik_enabled(instance) };
    assert!(!ik_ptr.is_null());
    let ik_slice = unsafe { std::slice::from_raw_parts(ik_ptr, 1) };

    // Copy API read
    let mut morph_copy = [0.0f32; 1];
    assert!(unsafe {
        mmd_runtime_instance_copy_morph_weights(instance, morph_copy.as_mut_ptr(), 1)
    });

    let mut ik_copy = [0u8; 1];
    assert!(unsafe { mmd_runtime_instance_copy_ik_enabled(instance, ik_copy.as_mut_ptr(), 1) });

    assert_eq!(morph_slice, &morph_copy);
    assert_eq!(ik_slice, &ik_copy);

    unsafe {
        mmd_runtime_clip_free(clip);
        mmd_runtime_instance_free(instance);
        mmd_runtime_model_free(model);
    }
}

#[test]
fn clip_frame_range_reports_all_track_frames() {
    let bone_tracks = [MmdRuntimeFfiBoneTrack {
        bone_index: 0,
        keyframe_offset: 0,
        keyframe_count: 1,
    }];
    let bone_keyframes = [MmdRuntimeFfiBoneKeyframe {
        frame: 30,
        position_xyz: [0.0, 0.0, 0.0],
        rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
    }];
    let morph_tracks = [MmdRuntimeFfiMorphTrack {
        morph_index: 0,
        keyframe_offset: 0,
        keyframe_count: 2,
    }];
    let morph_keyframes = [
        MmdRuntimeFfiMorphKeyframe {
            frame: 10,
            weight: 0.0,
        },
        MmdRuntimeFfiMorphKeyframe {
            frame: 60,
            weight: 1.0,
        },
    ];
    let property_keyframes = [MmdRuntimeFfiPropertyKeyframe {
        frame: 5,
        ik_enabled_offset: 0,
        ik_enabled_count: 1,
    }];
    let property_ik_enabled = [1_u8];
    let clip = unsafe {
        mmd_runtime_clip_create(
            bone_tracks.as_ptr(),
            bone_tracks.len(),
            bone_keyframes.as_ptr(),
            bone_keyframes.len(),
            morph_tracks.as_ptr(),
            morph_tracks.len(),
            morph_keyframes.as_ptr(),
            morph_keyframes.len(),
            property_keyframes.as_ptr(),
            property_keyframes.len(),
            property_ik_enabled.as_ptr(),
            property_ik_enabled.len(),
        )
    };
    assert!(!clip.is_null());

    let mut first = 0;
    let mut last = 0;
    assert!(unsafe { mmd_runtime_clip_frame_range(clip, &mut first, &mut last) });
    assert_eq!((first, last), (5, 60));

    unsafe {
        mmd_runtime_clip_free(clip);
    }
}

#[test]
fn clip_frame_range_rejects_null_or_empty() {
    let mut first = 99;
    let mut last = 99;
    assert!(!unsafe { mmd_runtime_clip_frame_range(ptr::null(), &mut first, &mut last) });
    assert_eq!((first, last), (99, 99));

    let empty_clip = unsafe {
        mmd_runtime_clip_create(
            ptr::null(),
            0,
            ptr::null(),
            0,
            ptr::null(),
            0,
            ptr::null(),
            0,
            ptr::null(),
            0,
            ptr::null(),
            0,
        )
    };
    assert!(!empty_clip.is_null());
    assert!(!unsafe { mmd_runtime_clip_frame_range(empty_clip, &mut first, &mut last) });
    assert!(!unsafe { mmd_runtime_clip_frame_range(empty_clip, ptr::null_mut(), &mut last) });
    assert!(!unsafe { mmd_runtime_clip_frame_range(empty_clip, &mut first, ptr::null_mut()) });

    unsafe {
        mmd_runtime_clip_free(empty_clip);
    }
}

// -----------------------------------------------------------------------
// PMX/VMD byte-import ABI tests (Phase 9)
// -----------------------------------------------------------------------

#[test]
fn import_pmx_bytes_rejects_null() {
    assert!(unsafe { mmd_runtime_model_create_from_pmx_bytes(ptr::null(), 0) }.is_null());
    assert!(unsafe { mmd_runtime_model_create_from_pmx_bytes(ptr::null(), 100) }.is_null());
    let dummy = 0u8;
    assert!(unsafe { mmd_runtime_model_create_from_pmx_bytes(&dummy as *const u8, 0) }.is_null());
}

#[test]
fn import_pmx_bytes_rejects_garbage() {
    let garbage = [0u8; 32];
    let model = unsafe { mmd_runtime_model_create_from_pmx_bytes(garbage.as_ptr(), garbage.len()) };
    assert!(model.is_null());
}

#[test]
fn import_vmd_bytes_for_model_rejects_null_and_empty() {
    let parents = [-1];
    let rest_positions = [0.0, 0.0, 0.0];
    let model = unsafe { mmd_runtime_model_create(parents.as_ptr(), rest_positions.as_ptr(), 1) };
    assert!(!model.is_null());

    // Null model
    assert!(
        unsafe { mmd_runtime_clip_create_from_vmd_bytes_for_model(ptr::null(), ptr::null(), 0) }
            .is_null()
    );
    assert_eq!(
        last_error_cstr().unwrap().to_bytes(),
        FFI_ERR_INVALID_INPUT.as_bytes()
    );
    // Null bytes
    assert!(
        unsafe { mmd_runtime_clip_create_from_vmd_bytes_for_model(model, ptr::null(), 100) }
            .is_null()
    );
    assert_eq!(
        last_error_cstr().unwrap().to_bytes(),
        FFI_ERR_INVALID_INPUT.as_bytes()
    );
    // Zero length
    let dummy = 0u8;
    assert!(
        unsafe { mmd_runtime_clip_create_from_vmd_bytes_for_model(model, &dummy as *const u8, 0) }
            .is_null()
    );
    assert_eq!(
        last_error_cstr().unwrap().to_bytes(),
        FFI_ERR_INVALID_INPUT.as_bytes()
    );

    unsafe {
        mmd_runtime_model_free(model);
    }
}

#[test]
fn flat_array_model_returns_null_from_vmd_import() {
    // Flat-array constructed models have empty name maps, so VMD import
    // should return null.
    let parents = [-1];
    let rest_positions = [0.0, 0.0, 0.0];
    let model = unsafe { mmd_runtime_model_create(parents.as_ptr(), rest_positions.as_ptr(), 1) };
    assert!(!model.is_null());

    let garbage = [0u8; 32];
    assert!(
        unsafe {
            mmd_runtime_clip_create_from_vmd_bytes_for_model(model, garbage.as_ptr(), garbage.len())
        }
        .is_null()
    );
    assert_eq!(
        last_error_cstr().unwrap().to_bytes(),
        FFI_ERR_CLIP_CREATE_FAILED.as_bytes()
    );

    unsafe {
        mmd_runtime_model_free(model);
    }
}

// -----------------------------------------------------------------------
//  JSON / geometry buffer API tests
// -----------------------------------------------------------------------

#[test]
fn vmd_json_rejects_null_empty_invalid() {
    let null_empty = unsafe { mmd_runtime_parse_vmd_json(ptr::null(), 0) };
    assert!(null_empty.data.is_null());
    assert_eq!(null_empty.len, 0);

    let null_nonempty = unsafe { mmd_runtime_parse_vmd_json(ptr::null(), 10) };
    assert!(null_nonempty.data.is_null());
    assert_eq!(null_nonempty.len, 0);

    let d = 0u8;
    let empty = unsafe { mmd_runtime_parse_vmd_json(&d as *const u8, 0) };
    assert!(empty.data.is_null());
    assert_eq!(empty.len, 0);

    let garbage = [0u8; 16];
    let invalid = unsafe { mmd_runtime_parse_vmd_json(garbage.as_ptr(), garbage.len()) };
    assert!(invalid.data.is_null());
    assert_eq!(invalid.len, 0);
}

#[test]
fn vmd_json_serializes_camera_fixture() {
    let bytes: &[u8] = include_bytes!("../../mmd-anim-format/fixtures/vmd/simple_camera.vmd");
    let json_buf = unsafe { mmd_runtime_parse_vmd_json(bytes.as_ptr(), bytes.len()) };
    assert!(!json_buf.data.is_null());
    assert!(json_buf.len > 0);

    let json_str =
        unsafe { str::from_utf8(slice::from_raw_parts(json_buf.data, json_buf.len)) }.unwrap();
    let v: serde_json::Value = serde_json::from_str(json_str).unwrap();
    assert!(v.is_object(), "vmd json must be an object");

    unsafe { mmd_runtime_byte_buffer_free(json_buf) };
}

#[test]
fn vmd_camera_track_samples_camera_fixture() {
    let bytes: &[u8] = include_bytes!("../../mmd-anim-format/fixtures/vmd/simple_camera.vmd");
    let track =
        unsafe { mmd_runtime_vmd_camera_track_create_from_vmd_bytes(bytes.as_ptr(), bytes.len()) };
    assert!(!track.is_null());
    assert_eq!(
        unsafe { mmd_runtime_vmd_camera_track_frame_count(track) },
        2
    );

    let mut values = [0.0f32; 9];
    assert!(unsafe {
        mmd_runtime_vmd_camera_track_sample(track, 22.5, values.as_mut_ptr(), values.len())
    });
    assert_slice_near(
        &values,
        &[-40.25, -0.25, 6.0, 1.625, -0.1, -0.1, 0.75, 47.5, 1.0],
        1.0e-4,
    );

    unsafe { mmd_runtime_vmd_camera_track_free(track) };
}

#[test]
fn vmd_camera_one_shot_samples_camera_fixture() {
    let bytes: &[u8] = include_bytes!("../../mmd-anim-format/fixtures/vmd/simple_camera.vmd");
    let mut values = [0.0f32; 9];
    assert!(unsafe {
        mmd_runtime_vmd_sample_camera(
            bytes.as_ptr(),
            bytes.len(),
            22.5,
            values.as_mut_ptr(),
            values.len(),
        )
    });
    assert_slice_near(
        &values,
        &[-40.25, -0.25, 6.0, 1.625, -0.1, -0.1, 0.75, 47.5, 1.0],
        1.0e-4,
    );
}

#[test]
fn vmd_light_track_and_one_shot_sample_buffers() {
    let bytes = light_and_self_shadow_vmd_bytes();
    let track =
        unsafe { mmd_runtime_vmd_light_track_create_from_vmd_bytes(bytes.as_ptr(), bytes.len()) };
    assert!(!track.is_null());
    assert_eq!(unsafe { mmd_runtime_vmd_light_track_frame_count(track) }, 2);

    let mut track_values = [0.0f32; 6];
    assert!(unsafe {
        mmd_runtime_vmd_light_track_sample(
            track,
            20.0,
            track_values.as_mut_ptr(),
            track_values.len(),
        )
    });
    assert_slice_near(&track_values, &[0.5, 0.25, 0.5, 0.5, -0.5, 0.0], 1.0e-4);

    let mut one_shot_values = [0.0f32; 6];
    assert!(unsafe {
        mmd_runtime_vmd_sample_light(
            bytes.as_ptr(),
            bytes.len(),
            20.0,
            one_shot_values.as_mut_ptr(),
            one_shot_values.len(),
        )
    });
    assert_slice_near(&one_shot_values, &track_values, 1.0e-4);

    unsafe { mmd_runtime_vmd_light_track_free(track) };
}

#[test]
fn vmd_self_shadow_track_and_one_shot_sample_buffers() {
    let bytes = light_and_self_shadow_vmd_bytes();
    let track = unsafe {
        mmd_runtime_vmd_self_shadow_track_create_from_vmd_bytes(bytes.as_ptr(), bytes.len())
    };
    assert!(!track.is_null());
    assert_eq!(
        unsafe { mmd_runtime_vmd_self_shadow_track_frame_count(track) },
        2
    );

    let mut track_values = [0.0f32; 2];
    assert!(unsafe {
        mmd_runtime_vmd_self_shadow_track_sample(
            track,
            20.0,
            track_values.as_mut_ptr(),
            track_values.len(),
        )
    });
    assert_slice_near(&track_values, &[1.0, 40.0], 1.0e-4);

    let mut one_shot_values = [0.0f32; 2];
    assert!(unsafe {
        mmd_runtime_vmd_sample_self_shadow(
            bytes.as_ptr(),
            bytes.len(),
            20.0,
            one_shot_values.as_mut_ptr(),
            one_shot_values.len(),
        )
    });
    assert_slice_near(&one_shot_values, &track_values, 1.0e-4);

    unsafe { mmd_runtime_vmd_self_shadow_track_free(track) };
}

#[test]
fn vmd_camera_sample_rejects_invalid_inputs() {
    assert!(
        unsafe { mmd_runtime_vmd_camera_track_create_from_vmd_bytes(ptr::null(), 0) }.is_null()
    );
    let mut values = [0.0f32; 8];
    assert!(!unsafe {
        mmd_runtime_vmd_camera_track_sample(ptr::null(), 0.0, values.as_mut_ptr(), values.len())
    });
    assert!(!unsafe { mmd_runtime_vmd_sample_camera(ptr::null(), 0, 0.0, values.as_mut_ptr(), 9) });
    assert!(!unsafe {
        mmd_runtime_vmd_sample_camera([0u8; 1].as_ptr(), 1, 0.0, values.as_mut_ptr(), values.len())
    });
}

fn light_and_self_shadow_vmd_bytes() -> Vec<u8> {
    mmd_anim_format::export_vmd_animation(&mmd_anim_format::vmd::VmdParsedAnimation {
        kind: "vmd",
        metadata: mmd_anim_format::vmd::VmdParsedMetadata {
            format: "vmd",
            model_name: "light_shadow".to_owned(),
            model_name_bytes: Vec::new(),
            counts: mmd_anim_format::vmd::VmdParsedCounts {
                bones: 0,
                morphs: 0,
                cameras: 0,
                lights: 2,
                self_shadows: 2,
                properties: 0,
            },
            max_frame: 30,
        },
        bone_frames: Vec::new(),
        morph_frames: Vec::new(),
        camera_frames: Vec::new(),
        light_frames: vec![
            mmd_anim_format::vmd::VmdParsedLightFrame {
                frame: 10,
                color: [0.0, 0.0, 1.0],
                direction: [1.0, 0.0, 0.0],
            },
            mmd_anim_format::vmd::VmdParsedLightFrame {
                frame: 30,
                color: [1.0, 0.5, 0.0],
                direction: [0.0, -1.0, 0.0],
            },
        ],
        self_shadow_frames: vec![
            mmd_anim_format::vmd::VmdParsedSelfShadowFrame {
                frame: 10,
                mode: 1,
                distance: 20.0,
            },
            mmd_anim_format::vmd::VmdParsedSelfShadowFrame {
                frame: 30,
                mode: 2,
                distance: 60.0,
            },
        ],
        property_frames: Vec::new(),
    })
}

#[test]
fn pmx_non_geometry_json_rejects_null_empty_invalid() {
    let null_empty = unsafe { mmd_runtime_parse_pmx_non_geometry_json(ptr::null(), 0) };
    assert!(null_empty.data.is_null());
    assert_eq!(null_empty.len, 0);

    let null_nonempty = unsafe { mmd_runtime_parse_pmx_non_geometry_json(ptr::null(), 10) };
    assert!(null_nonempty.data.is_null());
    assert_eq!(null_nonempty.len, 0);

    let d = 0u8;
    let empty = unsafe { mmd_runtime_parse_pmx_non_geometry_json(&d as *const u8, 0) };
    assert!(empty.data.is_null());
    assert_eq!(empty.len, 0);

    let garbage = [0u8; 16];
    let invalid =
        unsafe { mmd_runtime_parse_pmx_non_geometry_json(garbage.as_ptr(), garbage.len()) };
    assert!(invalid.data.is_null());
    assert_eq!(invalid.len, 0);
}

#[test]
fn pmx_non_geometry_json_omits_geometry_and_normalizes_fields() {
    let bytes: &[u8] = include_bytes!("../../mmd-anim-format/fixtures/pmx/ik_multi_axis_limit.pmx");
    let json_buf = unsafe { mmd_runtime_parse_pmx_non_geometry_json(bytes.as_ptr(), bytes.len()) };
    assert!(!json_buf.data.is_null());
    assert!(json_buf.len > 0);

    let json_str =
        unsafe { str::from_utf8(slice::from_raw_parts(json_buf.data, json_buf.len)) }.unwrap();
    let v: serde_json::Value = serde_json::from_str(json_str).unwrap();

    // geometry field must not be present
    assert!(v.get("geometry").is_none(), "geometry must be omitted");

    // required non-geometry fields must be present
    assert!(v.get("metadata").is_some());
    assert!(v.get("materials").is_some());
    assert!(v.get("skeleton").is_some());
    assert!(v.get("morphs").is_some());

    // sharedToonIndex null -> -1
    if let Some(mats) = v.get("materials").and_then(|m| m.as_array()) {
        for mat in mats {
            if let Some(idx) = mat.get("sharedToonIndex") {
                assert!(
                    !idx.is_null(),
                    "sharedToonIndex must not be null in output JSON"
                );
            }
        }
    }

    // externalParentKey null -> -1
    if let Some(bones) = v
        .get("skeleton")
        .and_then(|s| s.get("bones"))
        .and_then(|b| b.as_array())
    {
        for bone in bones {
            if let Some(key) = bone.get("externalParentKey") {
                assert!(
                    !key.is_null(),
                    "externalParentKey must not be null in output JSON"
                );
            }
        }
    }

    unsafe { mmd_runtime_byte_buffer_free(json_buf) };
}

#[test]
fn pmx_geometry_buffers_reject_null_empty_invalid() {
    macro_rules! check_rejects {
            ($fn:ident) => {{
                let null = unsafe { $fn(ptr::null(), 0) };
                assert!(null.data.is_null(), stringify!($fn null));
                assert_eq!(null.len, 0, stringify!($fn null len));

                let d = 0u8;
                let empty = unsafe { $fn(&d as *const u8, 0) };
                assert!(empty.data.is_null(), stringify!($fn empty));

                let garbage = [0u8; 16];
                let invalid = unsafe { $fn(garbage.as_ptr(), garbage.len()) };
                assert!(invalid.data.is_null(), stringify!($fn invalid));
            }};
        }

    check_rejects!(mmd_runtime_parse_pmx_positions_buffer);
    check_rejects!(mmd_runtime_parse_pmx_normals_buffer);
    check_rejects!(mmd_runtime_parse_pmx_uvs_buffer);
    check_rejects!(mmd_runtime_parse_pmx_indices_buffer);
    check_rejects!(mmd_runtime_parse_pmx_material_groups_buffer);
    check_rejects!(mmd_runtime_parse_pmx_skin_indices_buffer);
    check_rejects!(mmd_runtime_parse_pmx_skin_weights_buffer);
    check_rejects!(mmd_runtime_parse_pmx_edge_scale_buffer);
    check_rejects!(mmd_runtime_parse_pmx_sdef_enabled_buffer);
    check_rejects!(mmd_runtime_parse_pmx_sdef_c_buffer);
    check_rejects!(mmd_runtime_parse_pmx_sdef_r0_buffer);
    check_rejects!(mmd_runtime_parse_pmx_sdef_r1_buffer);
    check_rejects!(mmd_runtime_parse_pmx_sdef_rw0_buffer);
    check_rejects!(mmd_runtime_parse_pmx_sdef_rw1_buffer);
    check_rejects!(mmd_runtime_parse_pmx_qdef_enabled_buffer);
    check_rejects!(mmd_runtime_parse_pmx_skinning_modes_json);

    assert_eq!(
        unsafe { mmd_runtime_parse_pmx_additional_uv_count(ptr::null(), 0) },
        0
    );
    let d = 0u8;
    assert_eq!(
        unsafe { mmd_runtime_parse_pmx_additional_uv_count(&d as *const u8, 0) },
        0
    );
    let garbage = [0u8; 16];
    assert_eq!(
        unsafe { mmd_runtime_parse_pmx_additional_uv_count(garbage.as_ptr(), garbage.len()) },
        0
    );

    let null = unsafe { mmd_runtime_parse_pmx_additional_uvs_buffer(ptr::null(), 0, 0) };
    assert!(null.data.is_null(), "additional UV null");
    assert_eq!(null.len, 0, "additional UV null len");

    let empty = unsafe { mmd_runtime_parse_pmx_additional_uvs_buffer(&d as *const u8, 0, 0) };
    assert!(empty.data.is_null(), "additional UV empty");

    let invalid =
        unsafe { mmd_runtime_parse_pmx_additional_uvs_buffer(garbage.as_ptr(), garbage.len(), 0) };
    assert!(invalid.data.is_null(), "additional UV invalid");
}

fn ffi_buffer_to_vec(buffer: MmdRuntimeFfiByteBuffer) -> Vec<u8> {
    let bytes = if buffer.data.is_null() || buffer.len == 0 {
        Vec::new()
    } else {
        unsafe { slice::from_raw_parts(buffer.data, buffer.len).to_vec() }
    };
    unsafe { mmd_runtime_byte_buffer_free(buffer) };
    bytes
}

fn assert_empty_ffi_buffer(buffer: MmdRuntimeFfiByteBuffer, context: &str) {
    assert!(buffer.data.is_null(), "{context}: data must be null");
    assert_eq!(buffer.len, 0, "{context}: len must be zero");
}

fn assert_material_split_geometry_invariants(
    split: *mut MmdRuntimePmxMaterialSplit,
    manifest: &serde_json::Value,
    context: &str,
) {
    let mesh_count = unsafe { mmd_runtime_pmx_material_split_mesh_count(split) };
    assert!(mesh_count > 0, "{context}: mesh_count must be positive");
    assert_eq!(
        manifest.get("meshCount").and_then(|v| v.as_u64()),
        Some(mesh_count as u64),
        "{context}: manifest meshCount must match mesh_count"
    );

    let meshes = manifest
        .get("meshes")
        .and_then(|v| v.as_array())
        .unwrap_or_else(|| panic!("{context}: manifest meshes must be an array"));
    assert_eq!(
        meshes.len(),
        mesh_count,
        "{context}: manifest mesh array length must match mesh_count"
    );

    for mesh_index in 0..mesh_count {
        let mesh_context = format!("{context}: mesh {mesh_index}");
        let mesh_manifest = meshes
            .iter()
            .find(|mesh| mesh.get("meshIndex").and_then(|v| v.as_u64()) == Some(mesh_index as u64))
            .unwrap_or_else(|| panic!("{mesh_context}: manifest mesh entry missing"));

        let positions = ffi_buffer_to_vec(unsafe {
            mmd_runtime_pmx_material_split_positions_buffer(split, mesh_index)
        });
        assert_eq!(
            positions.len() % (3 * 4),
            0,
            "{mesh_context}: positions len must be xyz f32 aligned"
        );
        let vertex_count = positions.len() / (3 * 4);

        let normals = ffi_buffer_to_vec(unsafe {
            mmd_runtime_pmx_material_split_normals_buffer(split, mesh_index)
        });
        assert_eq!(
            normals.len(),
            positions.len(),
            "{mesh_context}: normals len"
        );

        let uvs = ffi_buffer_to_vec(unsafe {
            mmd_runtime_pmx_material_split_uvs_buffer(split, mesh_index)
        });
        assert_eq!(uvs.len(), vertex_count * 2 * 4, "{mesh_context}: uvs len");

        let skin_indices = ffi_buffer_to_vec(unsafe {
            mmd_runtime_pmx_material_split_skin_indices_buffer(split, mesh_index)
        });
        assert_eq!(
            skin_indices.len(),
            vertex_count * 4 * 4,
            "{mesh_context}: skin_indices len"
        );

        let skin_weights = ffi_buffer_to_vec(unsafe {
            mmd_runtime_pmx_material_split_skin_weights_buffer(split, mesh_index)
        });
        assert_eq!(
            skin_weights.len(),
            vertex_count * 4 * 4,
            "{mesh_context}: skin_weights len"
        );

        let edge_scale = ffi_buffer_to_vec(unsafe {
            mmd_runtime_pmx_material_split_edge_scale_buffer(split, mesh_index)
        });
        assert_eq!(
            edge_scale.len(),
            vertex_count * 4,
            "{mesh_context}: edge_scale len"
        );

        let sdef_enabled = ffi_buffer_to_vec(unsafe {
            mmd_runtime_pmx_material_split_sdef_enabled_buffer(split, mesh_index)
        });
        assert_eq!(
            sdef_enabled.len(),
            vertex_count,
            "{mesh_context}: sdef_enabled len"
        );

        let qdef_enabled = ffi_buffer_to_vec(unsafe {
            mmd_runtime_pmx_material_split_qdef_enabled_buffer(split, mesh_index)
        });
        assert_eq!(
            qdef_enabled.len(),
            vertex_count,
            "{mesh_context}: qdef_enabled len"
        );

        macro_rules! check_vec3_f32_buffer {
            ($fn:ident, $name:literal) => {{
                let buf = ffi_buffer_to_vec(unsafe { $fn(split, mesh_index) });
                assert_eq!(
                    buf.len(),
                    vertex_count * 3 * 4,
                    "{}: {} len",
                    mesh_context,
                    $name
                );
            }};
        }

        check_vec3_f32_buffer!(mmd_runtime_pmx_material_split_sdef_c_buffer, "sdef_c");
        check_vec3_f32_buffer!(mmd_runtime_pmx_material_split_sdef_r0_buffer, "sdef_r0");
        check_vec3_f32_buffer!(mmd_runtime_pmx_material_split_sdef_r1_buffer, "sdef_r1");
        check_vec3_f32_buffer!(mmd_runtime_pmx_material_split_sdef_rw0_buffer, "sdef_rw0");
        check_vec3_f32_buffer!(mmd_runtime_pmx_material_split_sdef_rw1_buffer, "sdef_rw1");

        let indices = ffi_buffer_to_vec(unsafe {
            mmd_runtime_pmx_material_split_indices_buffer(split, mesh_index)
        });
        assert_eq!(
            indices.len() % 4,
            0,
            "{mesh_context}: indices len must be u32 aligned"
        );
        for (index_offset, index_bytes) in indices.chunks_exact(4).enumerate() {
            let index = u32::from_ne_bytes(index_bytes.try_into().unwrap()) as usize;
            assert!(
                index < vertex_count,
                "{mesh_context}: index {index_offset} value {index} must be < vertex_count {vertex_count}"
            );
        }

        for uv_index in 0..4 {
            let additional_uvs = ffi_buffer_to_vec(unsafe {
                mmd_runtime_pmx_material_split_additional_uvs_buffer(split, mesh_index, uv_index)
            });
            if !additional_uvs.is_empty() {
                assert_eq!(
                    additional_uvs.len(),
                    vertex_count * 4 * 4,
                    "{mesh_context}: additional_uvs[{uv_index}] len"
                );
            }
        }

        let original_vertex_indices = mesh_manifest
            .get("originalVertexIndices")
            .and_then(|v| v.as_array())
            .unwrap_or_else(|| panic!("{mesh_context}: originalVertexIndices must be an array"));
        assert_eq!(
            original_vertex_indices.len(),
            vertex_count,
            "{mesh_context}: originalVertexIndices len"
        );

        let morph_index_map = mesh_manifest
            .get("morphIndexMap")
            .and_then(|v| v.as_array())
            .unwrap_or_else(|| panic!("{mesh_context}: morphIndexMap must be an array"));
        let mut seen_local_indices = vec![false; morph_index_map.len()];
        for entry in morph_index_map {
            let local_index = entry
                .get("localIndex")
                .and_then(|v| v.as_u64())
                .unwrap_or_else(|| panic!("{mesh_context}: localIndex missing"))
                as usize;
            assert!(
                local_index < morph_index_map.len(),
                "{mesh_context}: localIndex {local_index} out of range"
            );
            assert!(
                !seen_local_indices[local_index],
                "{mesh_context}: duplicate localIndex {local_index}"
            );
            seen_local_indices[local_index] = true;
        }
        assert!(
            seen_local_indices.iter().all(|seen| *seen),
            "{mesh_context}: localIndex values must be contiguous from zero"
        );
    }
}

fn assert_material_split_rejects_null_and_out_of_range(
    split: *mut MmdRuntimePmxMaterialSplit,
    mesh_count: usize,
) {
    assert_eq!(
        unsafe { mmd_runtime_pmx_material_split_mesh_count(ptr::null()) },
        0
    );
    assert_empty_ffi_buffer(
        unsafe { mmd_runtime_pmx_material_split_manifest_json(ptr::null()) },
        "null material split manifest",
    );

    macro_rules! check_empty_getter {
        ($fn:ident) => {{
            assert_empty_ffi_buffer(
                unsafe { $fn(ptr::null(), 0) },
                concat!(stringify!($fn), " null"),
            );
            assert_empty_ffi_buffer(
                unsafe { $fn(split, mesh_count) },
                concat!(stringify!($fn), " out of range"),
            );
        }};
    }

    check_empty_getter!(mmd_runtime_pmx_material_split_positions_buffer);
    check_empty_getter!(mmd_runtime_pmx_material_split_normals_buffer);
    check_empty_getter!(mmd_runtime_pmx_material_split_uvs_buffer);
    check_empty_getter!(mmd_runtime_pmx_material_split_indices_buffer);
    check_empty_getter!(mmd_runtime_pmx_material_split_skin_indices_buffer);
    check_empty_getter!(mmd_runtime_pmx_material_split_skin_weights_buffer);
    check_empty_getter!(mmd_runtime_pmx_material_split_edge_scale_buffer);
    check_empty_getter!(mmd_runtime_pmx_material_split_sdef_enabled_buffer);
    check_empty_getter!(mmd_runtime_pmx_material_split_sdef_c_buffer);
    check_empty_getter!(mmd_runtime_pmx_material_split_sdef_r0_buffer);
    check_empty_getter!(mmd_runtime_pmx_material_split_sdef_r1_buffer);
    check_empty_getter!(mmd_runtime_pmx_material_split_sdef_rw0_buffer);
    check_empty_getter!(mmd_runtime_pmx_material_split_sdef_rw1_buffer);
    check_empty_getter!(mmd_runtime_pmx_material_split_qdef_enabled_buffer);

    assert_empty_ffi_buffer(
        unsafe { mmd_runtime_pmx_material_split_additional_uvs_buffer(ptr::null(), 0, 0) },
        "additional_uvs null",
    );
    assert_empty_ffi_buffer(
        unsafe { mmd_runtime_pmx_material_split_additional_uvs_buffer(split, mesh_count, 0) },
        "additional_uvs out of range",
    );
}

fn material_split_manifest_json(
    split: *mut MmdRuntimePmxMaterialSplit,
    context: &str,
) -> serde_json::Value {
    let manifest_bytes =
        ffi_buffer_to_vec(unsafe { mmd_runtime_pmx_material_split_manifest_json(split) });
    assert!(
        !manifest_bytes.is_empty(),
        "{context}: manifest_json must not be empty"
    );
    serde_json::from_slice(&manifest_bytes)
        .unwrap_or_else(|err| panic!("{context}: manifest_json parse failed: {err}"))
}

fn rig_spec_manifest_json(spec: *mut MmdRuntimePmxRigSpec, context: &str) -> serde_json::Value {
    let manifest_bytes = ffi_buffer_to_vec(unsafe { mmd_runtime_pmx_rig_spec_manifest_json(spec) });
    assert!(
        !manifest_bytes.is_empty(),
        "{context}: manifest_json must not be empty"
    );
    serde_json::from_slice(&manifest_bytes)
        .unwrap_or_else(|err| panic!("{context}: manifest_json parse failed: {err}"))
}

fn assert_json_array3(value: &serde_json::Value, context: &str) {
    let array = value
        .as_array()
        .unwrap_or_else(|| panic!("{context}: must be an array"));
    assert_eq!(array.len(), 3, "{context}: must have three elements");
    assert!(
        array.iter().all(|item| item.is_number()),
        "{context}: elements must be numbers"
    );
}

#[test]
fn rig_spec_manifest_json_has_expected_shape() {
    let bytes: &[u8] = include_bytes!("../../mmd-anim-format/fixtures/pmx/ik_multi_axis_limit.pmx");
    let spec = unsafe { mmd_runtime_pmx_rig_spec_create(bytes.as_ptr(), bytes.len()) };
    assert!(!spec.is_null(), "rig spec handle must not be null");

    let manifest = rig_spec_manifest_json(spec, "fixture rig spec");
    let bone_count = manifest
        .get("boneCount")
        .and_then(|v| v.as_u64())
        .expect("fixture rig spec: boneCount must be a number");
    let ik_chain_count = manifest
        .get("ikChainCount")
        .and_then(|v| v.as_u64())
        .expect("fixture rig spec: ikChainCount must be a number");
    let grant_count = manifest
        .get("grantCount")
        .and_then(|v| v.as_u64())
        .expect("fixture rig spec: grantCount must be a number");

    let bones = manifest
        .get("bones")
        .and_then(|v| v.as_array())
        .expect("fixture rig spec: bones must be an array");
    let ik_chains = manifest
        .get("ikChains")
        .and_then(|v| v.as_array())
        .expect("fixture rig spec: ikChains must be an array");
    let grants = manifest
        .get("grants")
        .and_then(|v| v.as_array())
        .expect("fixture rig spec: grants must be an array");

    assert_eq!(bones.len(), bone_count as usize, "boneCount mismatch");
    assert_eq!(
        ik_chains.len(),
        ik_chain_count as usize,
        "ikChainCount mismatch"
    );
    assert_eq!(grants.len(), grant_count as usize, "grantCount mismatch");
    assert!(
        bone_count > 0,
        "fixture rig spec: boneCount must be positive"
    );
    assert!(
        ik_chain_count > 0,
        "fixture rig spec: ikChainCount must be positive"
    );

    for (bone_index, bone) in bones.iter().enumerate() {
        let context = format!("fixture rig spec: bone {bone_index}");
        assert!(
            bone.get("name").is_some_and(|v| v.is_string()),
            "{context}: name"
        );
        assert!(
            bone.get("nameBytes").is_some_and(|v| v.is_string()),
            "{context}: nameBytes"
        );
        assert!(
            bone.get("parentIndex").is_some_and(|v| v.is_number()),
            "{context}: parentIndex"
        );
        assert_json_array3(
            bone.get("restPosition")
                .unwrap_or_else(|| panic!("{context}: restPosition missing")),
            &format!("{context}: restPosition"),
        );
        assert!(
            bone.get("deformLayer").is_some_and(|v| v.is_number()),
            "{context}: deformLayer"
        );
        assert!(
            bone.get("fixedAxis").is_some(),
            "{context}: fixedAxis missing"
        );
        assert!(
            bone.get("localAxis").is_some(),
            "{context}: localAxis missing"
        );
        assert!(
            bone.get("transformAfterPhysics")
                .is_some_and(|v| v.is_boolean()),
            "{context}: transformAfterPhysics"
        );
        if let Some(local_axis) = bone.get("localAxis").filter(|v| !v.is_null()) {
            assert_json_array3(
                local_axis
                    .get("x")
                    .unwrap_or_else(|| panic!("{context}: localAxis.x missing")),
                &format!("{context}: localAxis.x"),
            );
            assert_json_array3(
                local_axis
                    .get("z")
                    .unwrap_or_else(|| panic!("{context}: localAxis.z missing")),
                &format!("{context}: localAxis.z"),
            );
        }
    }

    for (chain_index, chain) in ik_chains.iter().enumerate() {
        let context = format!("fixture rig spec: ik chain {chain_index}");
        assert!(
            chain
                .get("controllerBoneIndex")
                .is_some_and(|v| v.is_number()),
            "{context}: controllerBoneIndex"
        );
        assert!(
            chain.get("targetBoneIndex").is_some_and(|v| v.is_number()),
            "{context}: targetBoneIndex"
        );
        assert!(
            chain.get("iterationCount").is_some_and(|v| v.is_number()),
            "{context}: iterationCount"
        );
        assert!(
            chain.get("limitAngle").is_some_and(|v| v.is_number()),
            "{context}: limitAngle"
        );
        let links = chain
            .get("links")
            .and_then(|v| v.as_array())
            .unwrap_or_else(|| panic!("{context}: links must be an array"));
        for (link_index, link) in links.iter().enumerate() {
            let context = format!("{context}: link {link_index}");
            assert!(
                link.get("boneIndex").is_some_and(|v| v.is_number()),
                "{context}: boneIndex"
            );
            assert!(
                link.get("hasAngleLimit").is_some_and(|v| v.is_boolean()),
                "{context}: hasAngleLimit"
            );
            assert_json_array3(
                link.get("angleLimitMin")
                    .unwrap_or_else(|| panic!("{context}: angleLimitMin missing")),
                &format!("{context}: angleLimitMin"),
            );
            assert_json_array3(
                link.get("angleLimitMax")
                    .unwrap_or_else(|| panic!("{context}: angleLimitMax missing")),
                &format!("{context}: angleLimitMax"),
            );
        }
    }

    for (grant_index, grant) in grants.iter().enumerate() {
        let context = format!("fixture rig spec: grant {grant_index}");
        assert!(
            grant.get("targetBoneIndex").is_some_and(|v| v.is_number()),
            "{context}: targetBoneIndex"
        );
        assert!(
            grant.get("sourceBoneIndex").is_some_and(|v| v.is_number()),
            "{context}: sourceBoneIndex"
        );
        assert!(
            grant.get("ratio").is_some_and(|v| v.is_number()),
            "{context}: ratio"
        );
        assert!(
            grant.get("affectRotation").is_some_and(|v| v.is_boolean()),
            "{context}: affectRotation"
        );
        assert!(
            grant
                .get("affectTranslation")
                .is_some_and(|v| v.is_boolean()),
            "{context}: affectTranslation"
        );
        assert!(
            grant.get("local").is_some_and(|v| v.is_boolean()),
            "{context}: local"
        );
    }

    unsafe { mmd_runtime_pmx_rig_spec_free(spec) };
}

#[test]
fn rig_spec_rejects_null_and_invalid_input() {
    let null_spec = unsafe { mmd_runtime_pmx_rig_spec_create(ptr::null(), 1) };
    assert!(null_spec.is_null(), "null input must return null handle");

    let byte = 0_u8;
    let empty_spec = unsafe { mmd_runtime_pmx_rig_spec_create(&byte as *const u8, 0) };
    assert!(empty_spec.is_null(), "empty input must return null handle");

    let garbage = b"not a pmx";
    let invalid_spec = unsafe { mmd_runtime_pmx_rig_spec_create(garbage.as_ptr(), garbage.len()) };
    assert!(
        invalid_spec.is_null(),
        "invalid input must return null handle"
    );

    assert_empty_ffi_buffer(
        unsafe { mmd_runtime_pmx_rig_spec_manifest_json(ptr::null()) },
        "null rig spec manifest",
    );
    unsafe { mmd_runtime_pmx_rig_spec_free(ptr::null_mut()) };
}

#[test]
fn pmx_material_split_buffers_have_consistent_dimensions() {
    let bytes: &[u8] = include_bytes!("../../mmd-anim-format/fixtures/pmx/ik_multi_axis_limit.pmx");
    let split = unsafe { mmd_runtime_pmx_material_split_create(bytes.as_ptr(), bytes.len(), 0) };
    assert!(!split.is_null(), "material split handle must not be null");

    let mesh_count = unsafe { mmd_runtime_pmx_material_split_mesh_count(split) };
    let manifest = material_split_manifest_json(split, "fixture material split");
    assert_material_split_geometry_invariants(split, &manifest, "fixture material split");
    assert_material_split_rejects_null_and_out_of_range(split, mesh_count);

    unsafe { mmd_runtime_pmx_material_split_free(split) };
}

#[test]
fn pmx_geometry_buffers_have_correct_dimensions() {
    let bytes: &[u8] = include_bytes!("../../mmd-anim-format/fixtures/pmx/ik_multi_axis_limit.pmx");
    let parsed = mmd_anim_format::parse_pmx_model(bytes).unwrap();
    let vertex_count = parsed.metadata.counts.vertices;
    let index_count = parsed.metadata.counts.faces * 3;
    let additional_uv_count = parsed.geometry.additional_uvs.len();
    let material_group_count = parsed.geometry.material_groups.len();

    macro_rules! check_buf {
            ($fn:ident, $expected_bytes:expr) => {{
                let buf = unsafe { $fn(bytes.as_ptr(), bytes.len()) };
                assert!(!buf.data.is_null(), stringify!($fn must not be null));
                assert_eq!(
                    buf.len,
                    $expected_bytes,
                    stringify!($fn dimension mismatch)
                );
                unsafe { mmd_runtime_byte_buffer_free(buf) };
            }};
        }

    check_buf!(mmd_runtime_parse_pmx_positions_buffer, vertex_count * 3 * 4);
    check_buf!(mmd_runtime_parse_pmx_normals_buffer, vertex_count * 3 * 4);
    check_buf!(mmd_runtime_parse_pmx_uvs_buffer, vertex_count * 2 * 4);
    check_buf!(mmd_runtime_parse_pmx_indices_buffer, index_count * 4);
    check_buf!(
        mmd_runtime_parse_pmx_material_groups_buffer,
        material_group_count * 3 * 4
    );
    check_buf!(
        mmd_runtime_parse_pmx_skin_indices_buffer,
        vertex_count * 4 * 4
    );
    check_buf!(
        mmd_runtime_parse_pmx_skin_weights_buffer,
        vertex_count * 4 * 4
    );
    check_buf!(mmd_runtime_parse_pmx_edge_scale_buffer, vertex_count * 4);
    check_buf!(mmd_runtime_parse_pmx_sdef_enabled_buffer, vertex_count);
    check_buf!(mmd_runtime_parse_pmx_sdef_c_buffer, vertex_count * 3 * 4);
    check_buf!(mmd_runtime_parse_pmx_sdef_r0_buffer, vertex_count * 3 * 4);
    check_buf!(mmd_runtime_parse_pmx_sdef_r1_buffer, vertex_count * 3 * 4);
    check_buf!(mmd_runtime_parse_pmx_sdef_rw0_buffer, vertex_count * 3 * 4);
    check_buf!(mmd_runtime_parse_pmx_sdef_rw1_buffer, vertex_count * 3 * 4);
    check_buf!(mmd_runtime_parse_pmx_qdef_enabled_buffer, vertex_count);

    assert_eq!(
        unsafe { mmd_runtime_parse_pmx_additional_uv_count(bytes.as_ptr(), bytes.len()) },
        additional_uv_count
    );
    for uv_index in 0..additional_uv_count {
        let buf = unsafe {
            mmd_runtime_parse_pmx_additional_uvs_buffer(bytes.as_ptr(), bytes.len(), uv_index)
        };
        assert!(
            !buf.data.is_null(),
            "additional UV channel {uv_index} must not be null"
        );
        assert_eq!(
            buf.len,
            vertex_count * 4 * 4,
            "additional UV channel {uv_index} dimension mismatch"
        );
        unsafe { mmd_runtime_byte_buffer_free(buf) };
    }
    let invalid_uv = unsafe {
        mmd_runtime_parse_pmx_additional_uvs_buffer(
            bytes.as_ptr(),
            bytes.len(),
            additional_uv_count,
        )
    };
    assert!(invalid_uv.data.is_null(), "invalid additional UV index");
    assert_eq!(invalid_uv.len, 0, "invalid additional UV index len");
}

#[test]
fn pmx_geometry_handle_buffers_have_correct_dimensions() {
    let bytes: &[u8] = include_bytes!("../../mmd-anim-format/fixtures/pmx/ik_multi_axis_limit.pmx");
    let parsed = mmd_anim_format::parse_pmx_model(bytes).unwrap();
    let vertex_count = parsed.metadata.counts.vertices;
    let index_count = parsed.metadata.counts.faces * 3;
    let additional_uv_count = parsed.geometry.additional_uvs.len();
    let material_group_count = parsed.geometry.material_groups.len();

    let geometry = unsafe { mmd_runtime_pmx_geometry_create(bytes.as_ptr(), bytes.len()) };
    assert!(!geometry.is_null(), "geometry handle must not be null");

    macro_rules! check_buf {
        ($fn:ident, $expected_bytes:expr) => {{
            let buf = unsafe { $fn(geometry) };
            assert!(!buf.data.is_null(), stringify!($fn must not be null));
            assert_eq!(
                buf.len,
                $expected_bytes,
                stringify!($fn dimension mismatch)
            );
            unsafe { mmd_runtime_byte_buffer_free(buf) };
        }};
    }

    check_buf!(
        mmd_runtime_pmx_geometry_positions_buffer,
        vertex_count * 3 * 4
    );
    check_buf!(
        mmd_runtime_pmx_geometry_normals_buffer,
        vertex_count * 3 * 4
    );
    check_buf!(mmd_runtime_pmx_geometry_uvs_buffer, vertex_count * 2 * 4);
    check_buf!(mmd_runtime_pmx_geometry_indices_buffer, index_count * 4);
    check_buf!(
        mmd_runtime_pmx_geometry_material_groups_buffer,
        material_group_count * 3 * 4
    );
    check_buf!(
        mmd_runtime_pmx_geometry_skin_indices_buffer,
        vertex_count * 4 * 4
    );
    check_buf!(
        mmd_runtime_pmx_geometry_skin_weights_buffer,
        vertex_count * 4 * 4
    );
    check_buf!(mmd_runtime_pmx_geometry_edge_scale_buffer, vertex_count * 4);
    check_buf!(mmd_runtime_pmx_geometry_sdef_enabled_buffer, vertex_count);
    check_buf!(mmd_runtime_pmx_geometry_sdef_c_buffer, vertex_count * 3 * 4);
    check_buf!(
        mmd_runtime_pmx_geometry_sdef_r0_buffer,
        vertex_count * 3 * 4
    );
    check_buf!(
        mmd_runtime_pmx_geometry_sdef_r1_buffer,
        vertex_count * 3 * 4
    );
    check_buf!(
        mmd_runtime_pmx_geometry_sdef_rw0_buffer,
        vertex_count * 3 * 4
    );
    check_buf!(
        mmd_runtime_pmx_geometry_sdef_rw1_buffer,
        vertex_count * 3 * 4
    );
    check_buf!(mmd_runtime_pmx_geometry_qdef_enabled_buffer, vertex_count);

    assert_eq!(
        unsafe { mmd_runtime_pmx_geometry_additional_uv_count(geometry) },
        additional_uv_count
    );
    for uv_index in 0..additional_uv_count {
        let buf = unsafe { mmd_runtime_pmx_geometry_additional_uvs_buffer(geometry, uv_index) };
        assert!(
            !buf.data.is_null(),
            "geometry additional UV channel {uv_index} must not be null"
        );
        assert_eq!(
            buf.len,
            vertex_count * 4 * 4,
            "geometry additional UV channel {uv_index} dimension mismatch"
        );
        unsafe { mmd_runtime_byte_buffer_free(buf) };
    }

    assert_empty_ffi_buffer(
        unsafe { mmd_runtime_pmx_geometry_positions_buffer(ptr::null()) },
        "null PMX geometry positions",
    );
    assert_eq!(
        unsafe { mmd_runtime_pmx_geometry_additional_uv_count(ptr::null()) },
        0
    );
    assert_empty_ffi_buffer(
        unsafe { mmd_runtime_pmx_geometry_additional_uvs_buffer(geometry, additional_uv_count) },
        "invalid PMX geometry additional UV",
    );
    unsafe { mmd_runtime_pmx_geometry_free(geometry) };

    let invalid = unsafe { mmd_runtime_pmx_geometry_create(ptr::null(), 0) };
    assert!(
        invalid.is_null(),
        "invalid PMX geometry input must return null"
    );
    assert_eq!(
        last_error_cstr().unwrap().to_bytes(),
        FFI_ERR_INVALID_INPUT.as_bytes()
    );
    unsafe { mmd_runtime_pmx_geometry_free(ptr::null_mut()) };
}

#[test]
fn pmx_geometry_handle_buffers_match_legacy_raw_byte_api() {
    let bytes: &[u8] = include_bytes!("../../mmd-anim-format/fixtures/pmx/ik_multi_axis_limit.pmx");
    let parsed = mmd_anim_format::parse_pmx_model(bytes).unwrap();
    let additional_uv_count = parsed.geometry.additional_uvs.len();

    let geometry = unsafe { mmd_runtime_pmx_geometry_create(bytes.as_ptr(), bytes.len()) };
    assert!(!geometry.is_null(), "geometry handle must not be null");

    macro_rules! assert_same_buffer {
        ($legacy_fn:ident, $handle_fn:ident) => {{
            let legacy = ffi_buffer_to_vec(unsafe { $legacy_fn(bytes.as_ptr(), bytes.len()) });
            let handle = ffi_buffer_to_vec(unsafe { $handle_fn(geometry) });
            assert_eq!(handle, legacy, stringify!($handle_fn parity mismatch));
        }};
    }

    assert_same_buffer!(
        mmd_runtime_parse_pmx_positions_buffer,
        mmd_runtime_pmx_geometry_positions_buffer
    );
    assert_same_buffer!(
        mmd_runtime_parse_pmx_normals_buffer,
        mmd_runtime_pmx_geometry_normals_buffer
    );
    assert_same_buffer!(
        mmd_runtime_parse_pmx_uvs_buffer,
        mmd_runtime_pmx_geometry_uvs_buffer
    );
    assert_same_buffer!(
        mmd_runtime_parse_pmx_indices_buffer,
        mmd_runtime_pmx_geometry_indices_buffer
    );
    assert_same_buffer!(
        mmd_runtime_parse_pmx_material_groups_buffer,
        mmd_runtime_pmx_geometry_material_groups_buffer
    );
    assert_same_buffer!(
        mmd_runtime_parse_pmx_skin_indices_buffer,
        mmd_runtime_pmx_geometry_skin_indices_buffer
    );
    assert_same_buffer!(
        mmd_runtime_parse_pmx_skin_weights_buffer,
        mmd_runtime_pmx_geometry_skin_weights_buffer
    );
    assert_same_buffer!(
        mmd_runtime_parse_pmx_edge_scale_buffer,
        mmd_runtime_pmx_geometry_edge_scale_buffer
    );
    assert_same_buffer!(
        mmd_runtime_parse_pmx_sdef_enabled_buffer,
        mmd_runtime_pmx_geometry_sdef_enabled_buffer
    );
    assert_same_buffer!(
        mmd_runtime_parse_pmx_sdef_c_buffer,
        mmd_runtime_pmx_geometry_sdef_c_buffer
    );
    assert_same_buffer!(
        mmd_runtime_parse_pmx_sdef_r0_buffer,
        mmd_runtime_pmx_geometry_sdef_r0_buffer
    );
    assert_same_buffer!(
        mmd_runtime_parse_pmx_sdef_r1_buffer,
        mmd_runtime_pmx_geometry_sdef_r1_buffer
    );
    assert_same_buffer!(
        mmd_runtime_parse_pmx_sdef_rw0_buffer,
        mmd_runtime_pmx_geometry_sdef_rw0_buffer
    );
    assert_same_buffer!(
        mmd_runtime_parse_pmx_sdef_rw1_buffer,
        mmd_runtime_pmx_geometry_sdef_rw1_buffer
    );
    assert_same_buffer!(
        mmd_runtime_parse_pmx_qdef_enabled_buffer,
        mmd_runtime_pmx_geometry_qdef_enabled_buffer
    );

    assert_eq!(
        unsafe { mmd_runtime_pmx_geometry_additional_uv_count(geometry) },
        unsafe { mmd_runtime_parse_pmx_additional_uv_count(bytes.as_ptr(), bytes.len()) }
    );
    for uv_index in 0..additional_uv_count {
        let legacy = ffi_buffer_to_vec(unsafe {
            mmd_runtime_parse_pmx_additional_uvs_buffer(bytes.as_ptr(), bytes.len(), uv_index)
        });
        let handle = ffi_buffer_to_vec(unsafe {
            mmd_runtime_pmx_geometry_additional_uvs_buffer(geometry, uv_index)
        });
        assert_eq!(
            handle, legacy,
            "additional UV channel {uv_index} parity mismatch"
        );
    }

    unsafe { mmd_runtime_pmx_geometry_free(geometry) };
}

#[test]
fn pmx_skinning_modes_json_has_correct_shape() {
    let bytes: &[u8] = include_bytes!("../../mmd-anim-format/fixtures/pmx/ik_multi_axis_limit.pmx");
    let parsed = mmd_anim_format::parse_pmx_model(bytes).unwrap();
    let vertex_count = parsed.metadata.counts.vertices;

    let legacy_json = ffi_buffer_to_vec(unsafe {
        mmd_runtime_parse_pmx_skinning_modes_json(bytes.as_ptr(), bytes.len())
    });
    assert!(!legacy_json.is_empty());

    let geometry = unsafe { mmd_runtime_pmx_geometry_create(bytes.as_ptr(), bytes.len()) };
    assert!(!geometry.is_null(), "geometry handle must not be null");
    let handle_json =
        ffi_buffer_to_vec(unsafe { mmd_runtime_pmx_geometry_skinning_modes_json(geometry) });
    assert_eq!(
        handle_json, legacy_json,
        "handle skinning modes JSON must match legacy bytes API"
    );
    assert_empty_ffi_buffer(
        unsafe { mmd_runtime_pmx_geometry_skinning_modes_json(ptr::null()) },
        "null PMX geometry skinning modes",
    );
    unsafe { mmd_runtime_pmx_geometry_free(geometry) };

    let json_str = str::from_utf8(&legacy_json).unwrap();
    let v: serde_json::Value = serde_json::from_str(json_str).unwrap();

    let modes = v
        .get("skinningModes")
        .and_then(|m| m.as_array())
        .expect("skinningModes array must be present");
    assert_eq!(modes.len(), vertex_count);
    for mode in modes {
        let s = mode.as_str().expect("each skinning mode must be a string");
        assert!(
            matches!(s, "bdef1" | "bdef2" | "bdef4" | "sdef" | "qdef"),
            "unexpected skinning mode: {s}"
        );
    }
}

#[test]
fn pmx_skinning_modes_json_uses_parser_recorded_mode() {
    let geometry = mmd_anim_format::pmx::PmxParsedGeometry {
        positions: vec![0.0, 0.0, 0.0],
        normals: vec![0.0, 1.0, 0.0],
        uvs: vec![0.0, 0.0],
        additional_uvs: Vec::new(),
        indices: Vec::new(),
        skin_indices: vec![0, 1, 0, 0],
        skin_weights: vec![1.0, 0.0, 0.0, 0.0],
        edge_scale: vec![1.0],
        material_groups: Vec::new(),
        sdef: mmd_anim_format::pmx::PmxParsedSdef {
            skinning_modes: vec!["bdef2".to_owned()],
            enabled: vec![0.0],
            c: vec![0.0; 3],
            r0: vec![0.0; 3],
            r1: vec![0.0; 3],
            rw0: vec![0.0; 3],
            rw1: vec![0.0; 3],
        },
        qdef: mmd_anim_format::pmx::PmxParsedQdef { enabled: vec![0.0] },
    };

    let json = ffi_buffer_to_vec(pmx_skinning_modes_json_buffer(&geometry));
    let v: serde_json::Value = serde_json::from_slice(&json).unwrap();
    assert_eq!(v["skinningModes"][0], "bdef2");
}
