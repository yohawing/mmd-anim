use super::*;
use std::ffi::CStr;
use std::mem::MaybeUninit;

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

#[test]
fn descriptor_v1_layout_matches_shared_manifest() {
    if std::mem::size_of::<usize>() != 8 {
        // The manifest deliberately fixes only the supported Windows/Ubuntu
        // x86_64 ABI.  Keep other targets buildable without claiming parity.
        return;
    }
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("../abi/model_descriptor_v1.json"))
            .expect("valid model descriptor ABI manifest");
    assert_eq!(manifest["abi_version"].as_u64(), Some(ABI_VERSION as u64));
    assert_eq!(
        manifest["descriptor_version"].as_u64(),
        Some(MMD_RUNTIME_MODEL_DESCRIPTOR_VERSION_V1 as u64)
    );
    assert_eq!(
        manifest["feature"]["value"].as_u64(),
        Some(MMD_RUNTIME_FEATURE_MODEL_DESCRIPTOR as u64)
    );
    let manifest_flags = &manifest["flags"];
    for (name, value) in [
        (
            "MMD_RUNTIME_MODEL_BONE_TRANSFORM_AFTER_PHYSICS",
            MODEL_BONE_FLAG_TRANSFORM_AFTER_PHYSICS,
        ),
        (
            "MMD_RUNTIME_MODEL_BONE_FIXED_AXIS",
            MODEL_BONE_FLAG_FIXED_AXIS,
        ),
        (
            "MMD_RUNTIME_MODEL_BONE_LOCAL_AXIS",
            MODEL_BONE_FLAG_LOCAL_AXIS,
        ),
        (
            "MMD_RUNTIME_MODEL_IK_LINK_ANGLE_LIMIT",
            IK_LINK_FLAG_ANGLE_LIMIT,
        ),
        ("MMD_RUNTIME_APPEND_ROTATION", APPEND_FLAG_ROTATION),
        ("MMD_RUNTIME_APPEND_TRANSLATION", APPEND_FLAG_TRANSLATION),
        ("MMD_RUNTIME_APPEND_LOCAL", APPEND_FLAG_LOCAL),
        (
            "MMD_RUNTIME_MODEL_DESCRIPTOR_FLAGS_NONE",
            MMD_RUNTIME_MODEL_DESCRIPTOR_FLAGS_NONE,
        ),
    ] {
        assert_eq!(manifest_flags[name].as_u64(), Some(value as u64), "{name}");
    }
    let function = &manifest["functions"][0];
    assert_eq!(
        function["name"].as_str(),
        Some("mmd_runtime_model_create_from_descriptor")
    );
    assert_eq!(
        function["return_type"].as_str(),
        Some("mmd_runtime_model_t*")
    );
    assert_eq!(
        function["arguments"][0]["type"].as_str(),
        Some("const mmd_runtime_model_descriptor_t*")
    );
    assert_eq!(function["arguments"].as_array().map(Vec::len), Some(1));
    let _: unsafe extern "C" fn(*const MmdRuntimeModelDescriptor) -> *mut MmdRuntimeModel =
        mmd_runtime_model_create_from_descriptor;

    fn assert_record(
        manifest: &serde_json::Value,
        name: &str,
        size: usize,
        align: usize,
        fields: &[(&str, &str, usize)],
    ) {
        let record = manifest["records"]
            .as_array()
            .expect("records array")
            .iter()
            .find(|record| record["name"].as_str() == Some(name))
            .unwrap_or_else(|| panic!("manifest record missing: {name}"));
        assert_eq!(record["sizeof"].as_u64(), Some(size as u64), "{name} size");
        assert_eq!(
            record["alignof"].as_u64(),
            Some(align as u64),
            "{name} align"
        );
        let manifest_fields: Vec<(&str, &str, usize)> = record["fields"]
            .as_array()
            .expect("fields array")
            .iter()
            .map(|field| {
                (
                    field["name"].as_str().expect("field name"),
                    field["type"].as_str().expect("field type"),
                    field["offset"].as_u64().expect("field offset") as usize,
                )
            })
            .collect();
        assert_eq!(manifest_fields, fields, "{name} fields");
    }

    macro_rules! assert_record_types {
        ($name:literal, $record:ty, { $($field:ident: $rust_type:ty => $c_type:literal),+ $(,)? }) => {{
            $(let _: fn(&$record) -> &$rust_type = |value| &value.$field;)+
            assert_record(
                &manifest,
                $name,
                std::mem::size_of::<$record>(),
                std::mem::align_of::<$record>(),
                &[$((stringify!($field), $c_type, std::mem::offset_of!($record, $field))),+],
            );
        }};
    }

    assert_record_types!(
        "mmd_runtime_model_bone_descriptor_t",
        MmdRuntimeModelBoneDescriptor,
        {
            parent_index: i32 => "int32_t",
            rest_position_xyz: [f32; 3] => "float[3]",
            transform_order: i32 => "int32_t",
            flags: u32 => "uint32_t",
            fixed_axis_xyz: [f32; 3] => "float[3]",
            local_axis_x_xyz: [f32; 3] => "float[3]",
            local_axis_z_xyz: [f32; 3] => "float[3]",
        }
    );
    assert_record_types!(
        "mmd_runtime_model_ik_solver_descriptor_t",
        MmdRuntimeModelIkSolverDescriptor,
        {
            ik_bone_index: u32 => "uint32_t",
            target_bone_index: u32 => "uint32_t",
            link_offset: usize => "size_t",
            link_count: usize => "size_t",
            iteration_count: u32 => "uint32_t",
            limit_angle: f32 => "float",
        }
    );
    assert_record_types!(
        "mmd_runtime_model_ik_link_descriptor_t",
        MmdRuntimeModelIkLinkDescriptor,
        {
            bone_index: u32 => "uint32_t",
            flags: u32 => "uint32_t",
            angle_limit_min_xyz: [f32; 3] => "float[3]",
            angle_limit_max_xyz: [f32; 3] => "float[3]",
        }
    );
    assert_record_types!(
        "mmd_runtime_model_append_descriptor_t",
        MmdRuntimeModelAppendDescriptor,
        {
            target_bone_index: u32 => "uint32_t",
            source_bone_index: u32 => "uint32_t",
            ratio: f32 => "float",
            flags: u32 => "uint32_t",
        }
    );
    assert_record_types!(
        "mmd_runtime_model_bone_morph_offset_descriptor_t",
        MmdRuntimeModelBoneMorphOffsetDescriptor,
        {
            morph_index: u32 => "uint32_t",
            target_bone_index: u32 => "uint32_t",
            position_offset_xyz: [f32; 3] => "float[3]",
            rotation_offset_xyzw: [f32; 4] => "float[4]",
        }
    );
    assert_record_types!(
        "mmd_runtime_model_group_morph_offset_descriptor_t",
        MmdRuntimeModelGroupMorphOffsetDescriptor,
        {
            morph_index: u32 => "uint32_t",
            child_morph_index: u32 => "uint32_t",
            ratio: f32 => "float",
        }
    );
    assert_record_types!(
        "mmd_runtime_model_descriptor_t",
        MmdRuntimeModelDescriptor,
        {
            struct_size: u32 => "uint32_t",
            descriptor_version: u32 => "uint32_t",
            flags: u32 => "uint32_t",
            reserved: u32 => "uint32_t",
            bones: *const MmdRuntimeModelBoneDescriptor => "const mmd_runtime_model_bone_descriptor_t*",
            bone_count: usize => "size_t",
            ik_solvers: *const MmdRuntimeModelIkSolverDescriptor => "const mmd_runtime_model_ik_solver_descriptor_t*",
            ik_solver_count: usize => "size_t",
            ik_links: *const MmdRuntimeModelIkLinkDescriptor => "const mmd_runtime_model_ik_link_descriptor_t*",
            ik_link_count: usize => "size_t",
            append_transforms: *const MmdRuntimeModelAppendDescriptor => "const mmd_runtime_model_append_descriptor_t*",
            append_transform_count: usize => "size_t",
            morph_count: u32 => "uint32_t",
            bone_morph_offsets: *const MmdRuntimeModelBoneMorphOffsetDescriptor => "const mmd_runtime_model_bone_morph_offset_descriptor_t*",
            bone_morph_offset_count: usize => "size_t",
            group_morph_offsets: *const MmdRuntimeModelGroupMorphOffsetDescriptor => "const mmd_runtime_model_group_morph_offset_descriptor_t*",
            group_morph_offset_count: usize => "size_t",
        }
    );
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

fn reduced_pose_curve_fixture(target: u32) -> *mut MmdRuntimeReducedPose {
    let parents = [-1_i32];
    let rest = [0.0_f32, 0.0, 0.0];
    let model = unsafe { mmd_runtime_model_create(parents.as_ptr(), rest.as_ptr(), 1) };
    assert!(!model.is_null());

    let mut dense_world = Vec::new();
    for frame in 0..5 {
        let amount = frame as f32 / 4.0;
        let matrix = glam::Mat4::from_rotation_translation(
            glam::Quat::from_rotation_y(0.7 * amount * amount),
            glam::Vec3::new(amount, 0.25 * amount, 0.5 * amount),
        );
        dense_world.extend_from_slice(&matrix.to_cols_array());
    }
    let dense_morph = [0.0_f32, 0.1, 0.35, 0.7, 1.0];
    let tolerances = MmdRuntimeFfiReductionTolerances {
        local_position: 1.0e-5,
        local_rotation_radians: 1.0e-5,
        world_position: 1.0e-5,
        world_rotation_radians: 1.0e-5,
        morph_weight: 1.0e-5,
    };
    let mut reduced = ptr::null_mut();
    assert_eq!(
        unsafe {
            mmd_runtime_reduced_pose_create_from_dense(
                model,
                77,
                dense_world.as_ptr(),
                dense_world.len(),
                dense_morph.as_ptr(),
                dense_morph.len(),
                5,
                0.0,
                1.0,
                target,
                tolerances,
                &mut reduced,
            )
        },
        MmdRuntimeStatus::Ok
    );
    unsafe { mmd_runtime_model_free(model) };
    assert!(!reduced.is_null());
    reduced
}

fn copy_unity_curve_keys(
    reduced: *const MmdRuntimeReducedPose,
    frames_per_second: f32,
    flip_z: bool,
    curve_index: usize,
) -> Vec<MmdRuntimeFfiUnityCurveKey> {
    let mut required = 0;
    assert_eq!(
        unsafe {
            mmd_runtime_reduced_pose_unity_curve_keys(
                reduced,
                frames_per_second,
                u8::from(flip_z),
                curve_index,
                ptr::null_mut(),
                0,
                &mut required,
            )
        },
        MmdRuntimeStatus::BufferTooSmall
    );
    let mut keys = vec![MmdRuntimeFfiUnityCurveKey::default(); required];
    assert_eq!(
        unsafe {
            mmd_runtime_reduced_pose_unity_curve_keys(
                reduced,
                frames_per_second,
                u8::from(flip_z),
                curve_index,
                keys.as_mut_ptr(),
                keys.len(),
                &mut required,
            )
        },
        MmdRuntimeStatus::Ok
    );
    keys
}

#[test]
fn reduced_pose_unity_curves_are_two_call_and_match_rust_dto_after_model_free() {
    let reduced = reduced_pose_curve_fixture(2);
    let frames_per_second = 60.0;
    let flip_z = true;

    let mut curve_count = usize::MAX;
    assert_eq!(
        unsafe {
            mmd_runtime_reduced_pose_unity_curve_count(
                reduced,
                frames_per_second,
                u8::from(flip_z),
                &mut curve_count,
            )
        },
        MmdRuntimeStatus::Ok
    );
    assert_eq!(curve_count, 7);

    let sequence = unsafe { &(*reduced).sequence };
    let expected = mmd_anim_format::fbx::reduced_pose_to_unity_animation_clip_with_fps(
        sequence,
        &mmd_anim_format::fbx::UnityReducedPoseBindings {
            model_identity: 77,
            bone_paths: vec![String::new()],
            morph_bindings: vec![Some(mmd_anim_format::fbx::UnityMorphBinding {
                path: String::new(),
                property: String::new(),
            })],
        },
        frames_per_second,
        flip_z,
    )
    .unwrap();
    assert_eq!(expected.curves.len(), curve_count);

    for (curve_index, expected_curve) in expected.curves.iter().enumerate() {
        let mut descriptor = MmdRuntimeFfiUnityCurveDescriptor::default();
        assert_eq!(
            unsafe {
                mmd_runtime_reduced_pose_unity_curve_descriptor(
                    reduced,
                    frames_per_second,
                    u8::from(flip_z),
                    curve_index,
                    &mut descriptor,
                )
            },
            MmdRuntimeStatus::Ok
        );
        assert_eq!(descriptor.key_count, expected_curve.keys.len());
        if curve_index < 3 {
            assert_eq!(
                descriptor.semantic,
                MMD_RUNTIME_UNITY_CURVE_BONE_LOCAL_TRANSLATION
            );
            assert_eq!(descriptor.target_index, 0);
            assert_eq!(descriptor.axis, curve_index as u32);
        } else if curve_index < 6 {
            assert_eq!(
                descriptor.semantic,
                MMD_RUNTIME_UNITY_CURVE_BONE_LOCAL_EULER
            );
            assert_eq!(descriptor.target_index, 0);
            assert_eq!(descriptor.axis, (curve_index - 3) as u32);
        } else {
            assert_eq!(descriptor.semantic, MMD_RUNTIME_UNITY_CURVE_MORPH_WEIGHT);
            assert_eq!(descriptor.target_index, 0);
            assert_eq!(descriptor.axis, MMD_RUNTIME_UNITY_CURVE_AXIS_NONE);
        }

        let mut required = usize::MAX;
        assert_eq!(
            unsafe {
                mmd_runtime_reduced_pose_unity_curve_keys(
                    reduced,
                    frames_per_second,
                    u8::from(flip_z),
                    curve_index,
                    ptr::null_mut(),
                    0,
                    &mut required,
                )
            },
            MmdRuntimeStatus::BufferTooSmall
        );
        assert_eq!(required, descriptor.key_count);

        let mut keys = vec![MmdRuntimeFfiUnityCurveKey::default(); required];
        if required > 0 {
            assert_eq!(
                unsafe {
                    mmd_runtime_reduced_pose_unity_curve_keys(
                        reduced,
                        frames_per_second,
                        u8::from(flip_z),
                        curve_index,
                        keys.as_mut_ptr(),
                        required - 1,
                        &mut required,
                    )
                },
                MmdRuntimeStatus::BufferTooSmall
            );
        }
        assert_eq!(
            unsafe {
                mmd_runtime_reduced_pose_unity_curve_keys(
                    reduced,
                    frames_per_second,
                    u8::from(flip_z),
                    curve_index,
                    keys.as_mut_ptr(),
                    keys.len(),
                    &mut required,
                )
            },
            MmdRuntimeStatus::Ok
        );
        for (actual, expected) in keys.iter().zip(&expected_curve.keys) {
            assert_eq!(actual.time_seconds, expected.time_seconds);
            assert_eq!(actual.value, expected.value);
            assert_eq!(actual.in_tangent, expected.in_tangent);
            assert_eq!(actual.out_tangent, expected.out_tangent);
        }
    }

    let mut descriptor = MmdRuntimeFfiUnityCurveDescriptor::default();
    assert_eq!(
        unsafe {
            mmd_runtime_reduced_pose_unity_curve_descriptor(
                reduced,
                frames_per_second,
                u8::from(flip_z),
                curve_count,
                &mut descriptor,
            )
        },
        MmdRuntimeStatus::InvalidInput
    );
    unsafe { mmd_runtime_reduced_pose_free(reduced) };
}

#[test]
fn reduced_pose_unity_curves_validate_handle_fps_outputs_and_target() {
    let dcc = reduced_pose_curve_fixture(2);
    let mut count = usize::MAX;
    assert_eq!(
        unsafe { mmd_runtime_reduced_pose_unity_curve_count(ptr::null(), 30.0, 0, &mut count) },
        MmdRuntimeStatus::InvalidInput
    );
    assert_eq!(count, 0);
    assert_eq!(
        unsafe { mmd_runtime_reduced_pose_unity_curve_count(dcc, f32::NAN, 0, &mut count) },
        MmdRuntimeStatus::InvalidInput
    );
    assert_eq!(count, 0);
    assert_eq!(
        unsafe { mmd_runtime_reduced_pose_unity_curve_count(dcc, 0.0, 0, &mut count) },
        MmdRuntimeStatus::InvalidInput
    );
    for frames_per_second in [f32::MAX, f32::from_bits(1)] {
        let mut descriptor = MmdRuntimeFfiUnityCurveDescriptor {
            semantic: u32::MAX,
            target_index: u32::MAX,
            axis: u32::MAX,
            key_count: usize::MAX,
        };
        let mut required = usize::MAX;
        assert_eq!(
            unsafe {
                mmd_runtime_reduced_pose_unity_curve_count(dcc, frames_per_second, 0, &mut count)
            },
            MmdRuntimeStatus::InvalidInput
        );
        assert_eq!(count, 0);
        assert_eq!(
            unsafe {
                mmd_runtime_reduced_pose_unity_curve_descriptor(
                    dcc,
                    frames_per_second,
                    0,
                    0,
                    &mut descriptor,
                )
            },
            MmdRuntimeStatus::InvalidInput
        );
        assert_eq!(descriptor, MmdRuntimeFfiUnityCurveDescriptor::default());
        assert_eq!(
            unsafe {
                mmd_runtime_reduced_pose_unity_curve_keys(
                    dcc,
                    frames_per_second,
                    0,
                    0,
                    ptr::null_mut(),
                    0,
                    &mut required,
                )
            },
            MmdRuntimeStatus::InvalidInput
        );
        assert_eq!(required, 0);
    }
    assert_eq!(
        unsafe { mmd_runtime_reduced_pose_unity_curve_count(dcc, 30.0, 0, ptr::null_mut()) },
        MmdRuntimeStatus::InvalidInput
    );
    unsafe { mmd_runtime_reduced_pose_free(dcc) };

    let linear = reduced_pose_curve_fixture(0);
    assert_eq!(
        unsafe { mmd_runtime_reduced_pose_unity_curve_count(linear, 30.0, 0, &mut count) },
        MmdRuntimeStatus::Unsupported
    );
    assert_eq!(count, 0);
    unsafe { mmd_runtime_reduced_pose_free(linear) };
}

#[test]
fn reduced_pose_unity_curves_validate_raw_flip_z_flags() {
    let reduced = reduced_pose_curve_fixture(2);
    let mut keys_by_flag = Vec::new();

    for flip_z in [0_u8, 1] {
        let mut count = usize::MAX;
        assert_eq!(
            unsafe {
                mmd_runtime_reduced_pose_unity_curve_count(reduced, 30.0, flip_z, &mut count)
            },
            MmdRuntimeStatus::Ok
        );
        assert_eq!(count, 7);

        let mut descriptor = MmdRuntimeFfiUnityCurveDescriptor::default();
        assert_eq!(
            unsafe {
                mmd_runtime_reduced_pose_unity_curve_descriptor(
                    reduced,
                    30.0,
                    flip_z,
                    2,
                    &mut descriptor,
                )
            },
            MmdRuntimeStatus::Ok
        );
        assert_eq!(
            descriptor.semantic,
            MMD_RUNTIME_UNITY_CURVE_BONE_LOCAL_TRANSLATION
        );
        assert_eq!(descriptor.axis, MMD_RUNTIME_UNITY_CURVE_AXIS_Z);

        let mut required = usize::MAX;
        assert_eq!(
            unsafe {
                mmd_runtime_reduced_pose_unity_curve_keys(
                    reduced,
                    30.0,
                    flip_z,
                    2,
                    ptr::null_mut(),
                    0,
                    &mut required,
                )
            },
            MmdRuntimeStatus::BufferTooSmall
        );
        assert!(required > 0);
        let mut keys = vec![MmdRuntimeFfiUnityCurveKey::default(); required];
        assert_eq!(
            unsafe {
                mmd_runtime_reduced_pose_unity_curve_keys(
                    reduced,
                    30.0,
                    flip_z,
                    2,
                    keys.as_mut_ptr(),
                    keys.len(),
                    &mut required,
                )
            },
            MmdRuntimeStatus::Ok
        );
        keys_by_flag.push(keys);
    }

    assert!(
        keys_by_flag[0]
            .iter()
            .zip(&keys_by_flag[1])
            .any(|(unflipped, flipped)| unflipped.value != flipped.value)
    );

    let mut count = usize::MAX;
    assert_eq!(
        unsafe { mmd_runtime_reduced_pose_unity_curve_count(reduced, 30.0, 2, &mut count) },
        MmdRuntimeStatus::InvalidInput
    );
    assert_eq!(count, 0);

    let mut descriptor = MmdRuntimeFfiUnityCurveDescriptor {
        semantic: u32::MAX,
        target_index: u32::MAX,
        axis: u32::MAX,
        key_count: usize::MAX,
    };
    assert_eq!(
        unsafe {
            mmd_runtime_reduced_pose_unity_curve_descriptor(reduced, 30.0, 2, 2, &mut descriptor)
        },
        MmdRuntimeStatus::InvalidInput
    );
    assert_eq!(descriptor, MmdRuntimeFfiUnityCurveDescriptor::default());

    let sentinel = MmdRuntimeFfiUnityCurveKey {
        time_seconds: 9.0,
        value: 8.0,
        in_tangent: 7.0,
        out_tangent: 6.0,
    };
    let mut keys = [sentinel];
    let mut required = usize::MAX;
    assert_eq!(
        unsafe {
            mmd_runtime_reduced_pose_unity_curve_keys(
                reduced,
                30.0,
                2,
                2,
                keys.as_mut_ptr(),
                keys.len(),
                &mut required,
            )
        },
        MmdRuntimeStatus::InvalidInput
    );
    assert_eq!(required, 0);
    assert_eq!(keys, [sentinel]);

    unsafe { mmd_runtime_reduced_pose_free(reduced) };
}

#[test]
fn reduced_pose_unity_curves_initialize_maybe_uninit_outputs() {
    let reduced = reduced_pose_curve_fixture(2);

    let mut count = MaybeUninit::<usize>::uninit();
    assert_eq!(
        unsafe { mmd_runtime_reduced_pose_unity_curve_count(reduced, 30.0, 0, count.as_mut_ptr()) },
        MmdRuntimeStatus::Ok
    );
    assert_eq!(unsafe { count.assume_init() }, 7);

    let mut descriptor = MaybeUninit::<MmdRuntimeFfiUnityCurveDescriptor>::uninit();
    assert_eq!(
        unsafe {
            mmd_runtime_reduced_pose_unity_curve_descriptor(
                reduced,
                30.0,
                0,
                2,
                descriptor.as_mut_ptr(),
            )
        },
        MmdRuntimeStatus::Ok
    );
    let descriptor = unsafe { descriptor.assume_init() };
    assert!(descriptor.key_count > 0);

    let mut required = MaybeUninit::<usize>::uninit();
    assert_eq!(
        unsafe {
            mmd_runtime_reduced_pose_unity_curve_keys(
                reduced,
                30.0,
                0,
                2,
                ptr::null_mut(),
                0,
                required.as_mut_ptr(),
            )
        },
        MmdRuntimeStatus::BufferTooSmall
    );
    let required = unsafe { required.assume_init() };
    assert_eq!(required, descriptor.key_count);

    let mut keys = vec![MaybeUninit::<MmdRuntimeFfiUnityCurveKey>::uninit(); required];
    let mut written_required = MaybeUninit::<usize>::uninit();
    assert_eq!(
        unsafe {
            mmd_runtime_reduced_pose_unity_curve_keys(
                reduced,
                30.0,
                0,
                2,
                keys.as_mut_ptr().cast(),
                keys.len(),
                written_required.as_mut_ptr(),
            )
        },
        MmdRuntimeStatus::Ok
    );
    assert_eq!(unsafe { written_required.assume_init() }, required);
    let keys = keys
        .into_iter()
        .map(|key| unsafe { key.assume_init() })
        .collect::<Vec<_>>();
    assert_eq!(keys.len(), required);
    assert!(keys.iter().all(|key| {
        key.time_seconds.is_finite()
            && key.value.is_finite()
            && key.in_tangent.is_finite()
            && key.out_tangent.is_finite()
    }));

    unsafe { mmd_runtime_reduced_pose_free(reduced) };
}

#[test]
fn reduced_pose_unity_curve_cache_invalidates_for_fps_and_flip_z() {
    let reduced = reduced_pose_curve_fixture(2);
    let z_30 = copy_unity_curve_keys(reduced, 30.0, false, 2);
    let z_60 = copy_unity_curve_keys(reduced, 60.0, false, 2);
    let z_60_flipped = copy_unity_curve_keys(reduced, 60.0, true, 2);
    let z_30_again = copy_unity_curve_keys(reduced, 30.0, false, 2);

    assert_eq!(z_30_again, z_30);
    assert_near(z_30.last().unwrap().time_seconds, 4.0 / 30.0, 1.0e-7);
    assert_near(z_60.last().unwrap().time_seconds, 4.0 / 60.0, 1.0e-7);
    assert_near(z_30.last().unwrap().value, 0.5, 1.0e-6);
    assert_near(z_60_flipped.last().unwrap().value, -0.5, 1.0e-6);

    let tangent_index = z_30
        .iter()
        .position(|key| key.out_tangent.abs() > 1.0e-5)
        .expect("fixture must contain a non-zero translation tangent");
    assert_near(
        z_60[tangent_index].out_tangent,
        z_30[tangent_index].out_tangent * 2.0,
        1.0e-5,
    );
    assert_near(
        z_60_flipped[tangent_index].out_tangent,
        -z_60[tangent_index].out_tangent,
        1.0e-5,
    );

    unsafe { mmd_runtime_reduced_pose_free(reduced) };
}

#[test]
fn reduced_pose_handle_reports_and_enumerates_sparse_curves_after_model_free() {
    let parents = [-1_i32];
    let rest = [0.0_f32, 0.0, 0.0];
    let model = unsafe { mmd_runtime_model_create(parents.as_ptr(), rest.as_ptr(), 1) };
    assert!(!model.is_null());

    let mut dense_world = Vec::new();
    for x in [0.0_f32, 1.0, 2.0] {
        dense_world
            .extend_from_slice(&glam::Mat4::from_translation(glam::Vec3::X * x).to_cols_array());
    }
    let dense_morph = [0.0_f32, 0.5, 1.0];
    let tolerances = MmdRuntimeFfiReductionTolerances {
        local_position: 1.0e-4,
        local_rotation_radians: 1.0e-4,
        world_position: 1.0e-4,
        world_rotation_radians: 1.0e-4,
        morph_weight: 1.0e-4,
    };
    let mut reduced = ptr::null_mut();
    let status = unsafe {
        mmd_runtime_reduced_pose_create_from_dense(
            model,
            42,
            dense_world.as_ptr(),
            dense_world.len(),
            dense_morph.as_ptr(),
            dense_morph.len(),
            3,
            0.0,
            30.0,
            2,
            tolerances,
            &mut reduced,
        )
    };
    assert_eq!(status, MmdRuntimeStatus::Ok);
    assert!(!reduced.is_null());

    let mut invalid = std::ptr::NonNull::<MmdRuntimeReducedPose>::dangling().as_ptr();
    assert_eq!(
        unsafe {
            mmd_runtime_reduced_pose_create_from_dense(
                model,
                42,
                dense_world.as_ptr(),
                dense_world.len(),
                dense_morph.as_ptr(),
                dense_morph.len(),
                3,
                0.0,
                30.0,
                99,
                tolerances,
                &mut invalid,
            )
        },
        MmdRuntimeStatus::InvalidInput
    );
    assert!(invalid.is_null());
    unsafe { mmd_runtime_model_free(model) };

    let mut report = MmdRuntimeFfiPoseReductionReport::default();
    assert_eq!(
        unsafe { mmd_runtime_reduced_pose_report(reduced, &mut report) },
        MmdRuntimeStatus::Ok
    );
    assert_eq!(report.source_bone_key_count, 3);
    assert_eq!(report.reduced_bone_key_count, 2);
    assert_eq!(report.source_morph_key_count, 3);
    assert_eq!(report.reduced_morph_key_count, 2);

    let mut curve_count = 0;
    assert_eq!(
        unsafe { mmd_runtime_reduced_pose_unity_curve_count(reduced, 30.0, 0, &mut curve_count) },
        MmdRuntimeStatus::Ok
    );
    assert_eq!(curve_count, 7);
    let morph_keys = copy_unity_curve_keys(reduced, 30.0, false, curve_count - 1);
    assert_eq!(morph_keys.len(), 2);
    assert_near(morph_keys[0].value, 0.0, 1.0e-5);
    assert_near(morph_keys[1].value, 100.0, 1.0e-5);
    unsafe { mmd_runtime_reduced_pose_free(reduced) };
    unsafe { mmd_runtime_reduced_pose_free(ptr::null_mut()) };
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
        has_angle_limit: 0,
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
fn primitive_creators_reject_noncanonical_boolean_bytes() {
    let invalid_append = MmdRuntimeFfiAppendConfig {
        ratio: 1.0,
        affect_rotation: 2,
        affect_translation: 1,
    };
    assert!(unsafe { mmd_runtime_append_solver_create(&invalid_append) }.is_null());

    let bones = [MmdRuntimeFfiRigBone {
        parent_slot: -1,
        rest_position_xyz: [0.0, 0.0, 0.0],
        flags: 0,
        fixed_axis_xyz: [0.0, 0.0, 0.0],
    }];
    let invalid_link = [MmdRuntimeFfiRigIkLink {
        bone_slot: 0,
        has_angle_limit: 2,
        angle_limit_min_xyz: [0.0; 3],
        angle_limit_max_xyz: [0.0; 3],
    }];
    assert!(
        unsafe {
            mmd_runtime_ik_chain_create(
                bones.as_ptr(),
                bones.len(),
                0,
                invalid_link.as_ptr(),
                invalid_link.len(),
                1,
                0.0,
            )
        }
        .is_null()
    );

    let valid_link = [MmdRuntimeFfiRigIkLink {
        bone_slot: 0,
        has_angle_limit: 0,
        angle_limit_min_xyz: [0.0; 3],
        angle_limit_max_xyz: [0.0; 3],
    }];
    let invalid_local_axis = [MmdRuntimeFfiRigBoneLocalAxisV2 {
        has_local_axis: 2,
        local_axis_x_xyz: [0.0; 3],
        local_axis_z_xyz: [0.0; 3],
    }];
    assert!(
        unsafe {
            mmd_runtime_ik_chain_create_v2(
                bones.as_ptr(),
                bones.len(),
                invalid_local_axis.as_ptr(),
                0,
                valid_link.as_ptr(),
                valid_link.len(),
                1,
                0.0,
            )
        }
        .is_null()
    );
}

#[test]
fn append_solver_lifecycle_and_expected_output_use_xyzw_quaternion() {
    let config = MmdRuntimeFfiAppendConfig {
        ratio: 0.5,
        affect_rotation: 1,
        affect_translation: 1,
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
        affect_rotation: 1,
        affect_translation: 1,
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
fn ik_chain_create_v2_null_local_axes_matches_v1() {
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
        has_angle_limit: 0,
        angle_limit_min_xyz: [0.0, 0.0, 0.0],
        angle_limit_max_xyz: [0.0, 0.0, 0.0],
    }];
    let v1 = unsafe {
        mmd_runtime_ik_chain_create(
            bones.as_ptr(),
            bones.len(),
            1,
            links.as_ptr(),
            links.len(),
            4,
            0.0,
        )
    };
    let v2 = unsafe {
        mmd_runtime_ik_chain_create_v2(
            bones.as_ptr(),
            bones.len(),
            ptr::null(),
            1,
            links.as_ptr(),
            links.len(),
            4,
            0.0,
        )
    };
    assert!(!v1.is_null());
    assert!(!v2.is_null());

    let local_rotations = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    let goal = [0.0, 1.0, 0.0];
    let mut out_v1 = [0.0; 4];
    let mut out_v2 = [0.0; 4];
    assert!(unsafe {
        mmd_runtime_ik_chain_solve(
            v1,
            ptr::null(),
            ptr::null(),
            local_rotations.as_ptr(),
            goal.as_ptr(),
            0.0,
            0,
            out_v1.as_mut_ptr(),
            out_v1.len(),
            ptr::null_mut(),
        )
    });
    assert!(unsafe {
        mmd_runtime_ik_chain_solve(
            v2,
            ptr::null(),
            ptr::null(),
            local_rotations.as_ptr(),
            goal.as_ptr(),
            0.0,
            0,
            out_v2.as_mut_ptr(),
            out_v2.len(),
            ptr::null_mut(),
        )
    });
    assert_slice_near(&out_v1, &out_v2, 1.0e-6);
    unsafe {
        mmd_runtime_ik_chain_free(v1);
        mmd_runtime_ik_chain_free(v2);
    }
}

#[test]
fn ik_chain_create_v2_local_axis_changes_limited_solve() {
    // Pure X-axis limit in a Y-rotated local-axis frame behaves differently
    // from unit XYZ, matching the runtime local-axis angle-limit path.
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
    let half_pi = std::f32::consts::FRAC_PI_2;
    let links = [MmdRuntimeFfiRigIkLink {
        bone_slot: 0,
        has_angle_limit: 1,
        angle_limit_min_xyz: [-half_pi, 0.0, 0.0],
        angle_limit_max_xyz: [half_pi, 0.0, 0.0],
    }];
    // localAxis x=(0,0,1), z=(0,1,0) rebuilds a non-identity LA frame.
    let local_axes = [
        MmdRuntimeFfiRigBoneLocalAxisV2 {
            has_local_axis: 1,
            local_axis_x_xyz: [0.0, 0.0, 1.0],
            local_axis_z_xyz: [0.0, 1.0, 0.0],
        },
        MmdRuntimeFfiRigBoneLocalAxisV2 {
            has_local_axis: 0,
            local_axis_x_xyz: [0.0, 0.0, 0.0],
            local_axis_z_xyz: [0.0, 0.0, 0.0],
        },
    ];
    let unit = unsafe {
        mmd_runtime_ik_chain_create(
            bones.as_ptr(),
            bones.len(),
            1,
            links.as_ptr(),
            links.len(),
            4,
            0.0,
        )
    };
    let la = unsafe {
        mmd_runtime_ik_chain_create_v2(
            bones.as_ptr(),
            bones.len(),
            local_axes.as_ptr(),
            1,
            links.as_ptr(),
            links.len(),
            4,
            0.0,
        )
    };
    assert!(!unit.is_null());
    assert!(!la.is_null());

    let local_rotations = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    let goal = [0.0, 1.0, 0.0];
    let mut out_unit = [0.0; 4];
    let mut out_la = [0.0; 4];
    assert!(unsafe {
        mmd_runtime_ik_chain_solve(
            unit,
            ptr::null(),
            ptr::null(),
            local_rotations.as_ptr(),
            goal.as_ptr(),
            0.0,
            0,
            out_unit.as_mut_ptr(),
            out_unit.len(),
            ptr::null_mut(),
        )
    });
    assert!(unsafe {
        mmd_runtime_ik_chain_solve(
            la,
            ptr::null(),
            ptr::null(),
            local_rotations.as_ptr(),
            goal.as_ptr(),
            0.0,
            0,
            out_la.as_mut_ptr(),
            out_la.len(),
            ptr::null_mut(),
        )
    });

    let unit_q = glam::Quat::from_xyzw(out_unit[0], out_unit[1], out_unit[2], out_unit[3]);
    let la_q = glam::Quat::from_xyzw(out_la[0], out_la[1], out_la[2], out_la[3]);
    let unit_dir = unit_q.mul_vec3(glam::Vec3::X);
    let la_dir = la_q.mul_vec3(glam::Vec3::X);
    assert!(
        (unit_dir - la_dir).length() > 0.15,
        "v2 localAxis must change limited solve; unit={unit_dir:?} la={la_dir:?}"
    );

    unsafe {
        mmd_runtime_ik_chain_free(unit);
        mmd_runtime_ik_chain_free(la);
    }
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
fn split_physics_feature_flags_and_mode_config_work_through_c_abi() {
    assert_eq!(
        mmd_runtime_feature_flags() & FEATURE_SPLIT_PHYSICS_EVALUATION,
        FEATURE_SPLIT_PHYSICS_EVALUATION
    );

    let parents = [-1];
    let rest_positions = [0.0, 0.0, 0.0];
    let model = unsafe { mmd_runtime_model_create(parents.as_ptr(), rest_positions.as_ptr(), 1) };
    assert!(!model.is_null());
    let instance = unsafe { mmd_runtime_instance_create(model, 0) };
    assert!(!instance.is_null());

    let mut mode = MmdRuntimeFfiPhysicsMode::Live;
    assert_eq!(
        unsafe { mmd_runtime_instance_get_physics_mode(instance, &mut mode) },
        MmdRuntimeStatus::Ok
    );
    assert_eq!(mode, MmdRuntimeFfiPhysicsMode::Off);
    assert_eq!(
        unsafe {
            mmd_runtime_instance_set_physics_mode(instance, MmdRuntimeFfiPhysicsMode::Trace as u32)
        },
        MmdRuntimeStatus::Ok
    );
    assert_eq!(
        unsafe { mmd_runtime_instance_get_physics_mode(instance, &mut mode) },
        MmdRuntimeStatus::Ok
    );
    assert_eq!(mode, MmdRuntimeFfiPhysicsMode::Trace);

    let config = MmdRuntimeFfiPhysicsTickConfig {
        fixed_substep_seconds: 0.25,
        max_substeps_per_tick: 2,
    };
    assert_eq!(
        unsafe { mmd_runtime_instance_set_physics_tick_config(instance, &config) },
        MmdRuntimeStatus::Ok
    );
    let mut roundtrip = MmdRuntimeFfiPhysicsTickConfig {
        fixed_substep_seconds: 0.0,
        max_substeps_per_tick: 0,
    };
    assert_eq!(
        unsafe { mmd_runtime_instance_get_physics_tick_config(instance, &mut roundtrip) },
        MmdRuntimeStatus::Ok
    );
    assert_eq!(roundtrip, config);

    let mut stats = MmdRuntimeFfiPhysicsStepStats {
        input_dt_seconds: 0.0,
        clamped_dt_seconds: 0.0,
        substeps: 0,
        accumulator_seconds: 0.0,
    };
    assert_eq!(
        unsafe { mmd_runtime_instance_advance_physics_tick_clock(instance, 1.0, &mut stats) },
        MmdRuntimeStatus::Ok
    );
    assert_eq!(stats.input_dt_seconds, 1.0);
    assert_eq!(stats.clamped_dt_seconds, 0.5);
    assert_eq!(stats.substeps, 2);
    assert_near(stats.accumulator_seconds, 0.0, 1.0e-6);
    assert_eq!(
        unsafe { mmd_runtime_instance_reset_physics_tick(instance) },
        MmdRuntimeStatus::Ok
    );

    assert_eq!(
        unsafe { mmd_runtime_instance_get_physics_mode(ptr::null(), &mut mode) },
        MmdRuntimeStatus::InvalidInput
    );
    assert_eq!(
        unsafe { mmd_runtime_instance_set_physics_mode(instance, 99) },
        MmdRuntimeStatus::InvalidInput
    );
    assert_eq!(
        unsafe { mmd_runtime_instance_get_physics_tick_config(instance, ptr::null_mut()) },
        MmdRuntimeStatus::InvalidInput
    );

    unsafe {
        mmd_runtime_instance_free(instance);
        mmd_runtime_model_free(model);
    }
}

#[cfg(not(feature = "physics-bullet-native"))]
#[test]
fn physics_world_abi_exports_unsupported_stubs_when_feature_is_off() {
    assert_eq!(
        mmd_runtime_feature_flags() & FEATURE_PHYSICS_BULLET_NATIVE,
        0
    );

    let mut world = ptr::null_mut();
    assert_eq!(
        unsafe { mmd_runtime_physics_world_create(ptr::null(), 0, ptr::null(), 0, &mut world) },
        MmdRuntimeStatus::Unsupported
    );
    assert!(world.is_null());
    assert_eq!(
        unsafe { mmd_runtime_physics_world_create_from_pmx_bytes(ptr::null(), 0, &mut world) },
        MmdRuntimeStatus::Unsupported
    );
    assert_eq!(
        unsafe {
            mmd_runtime_physics_world_reset(ptr::null_mut(), ptr::null_mut(), ptr::null_mut())
        },
        MmdRuntimeStatus::Unsupported
    );
    assert_eq!(
        unsafe {
            mmd_runtime_physics_world_step_runtime(
                ptr::null_mut(),
                ptr::null_mut(),
                1.0 / 60.0,
                ptr::null_mut(),
            )
        },
        MmdRuntimeStatus::Unsupported
    );
    assert_eq!(
        unsafe {
            mmd_runtime_physics_world_bake_clip_frames(
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null(),
                0.0,
                1.0,
                1.0 / 60.0,
                0,
                ptr::null_mut(),
                0,
                ptr::null_mut(),
                0,
                ptr::null_mut(),
            )
        },
        MmdRuntimeStatus::Unsupported
    );
    assert_eq!(
        unsafe {
            mmd_runtime_physics_world_copy_rigidbody_bindings(
                ptr::null(),
                ptr::null_mut(),
                0,
                ptr::null_mut(),
            )
        },
        MmdRuntimeStatus::Unsupported
    );
    assert_eq!(
        unsafe {
            mmd_runtime_physics_world_physics_driven_bone_mask(ptr::null(), ptr::null_mut(), 0)
        },
        MmdRuntimeStatus::Unsupported
    );

    let get = unsafe { mmd_runtime_physics_params_get_json(ptr::null()) };
    assert!(get.data.is_null());
    assert_eq!(get.len, 0);
    assert_eq!(
        unsafe { mmd_runtime_physics_params_set_json(ptr::null_mut(), b"{}".as_ptr(), 2) },
        MmdRuntimeStatus::InvalidInput
    );

    let world = Box::into_raw(Box::new(MmdRuntimePhysicsWorld {}));
    let get = unsafe { mmd_runtime_physics_params_get_json(world) };
    assert!(get.data.is_null());
    assert_eq!(get.len, 0);
    assert_eq!(
        unsafe { mmd_runtime_physics_params_set_json(world, b"{}".as_ptr(), 2) },
        MmdRuntimeStatus::Unsupported
    );
    unsafe { mmd_runtime_physics_world_free(world) };
}

#[cfg(feature = "physics-bullet-native")]
fn dynamic_physics_body_desc() -> MmdRuntimeFfiPhysicsRigidBodyDesc {
    MmdRuntimeFfiPhysicsRigidBodyDesc {
        shape: MmdRuntimeFfiPhysicsRigidBodyShape::Sphere as u32,
        shape_size: [0.5, 0.0, 0.0],
        position_xyz: [0.0, 8.0, 0.0],
        rotation_euler_xyz: [0.0; 3],
        mass: 1.0,
        linear_damping: 0.0,
        angular_damping: 0.0,
        friction: 0.5,
        restitution: 0.0,
        collision_group: 0,
        collision_mask: 0xffff,
        bone_index: 0,
        mode: MmdRuntimeFfiPhysicsRigidBodyMode::Dynamic as u32,
        body_from_bone_position_xyz: [0.0; 3],
        body_from_bone_rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
        bone_from_body_position_xyz: [0.0; 3],
        bone_from_body_rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
    }
}

#[cfg(feature = "physics-bullet-native")]
fn dynamic_bone_physics_body_desc() -> MmdRuntimeFfiPhysicsRigidBodyDesc {
    MmdRuntimeFfiPhysicsRigidBodyDesc {
        mode: MmdRuntimeFfiPhysicsRigidBodyMode::DynamicBone as u32,
        ..dynamic_physics_body_desc()
    }
}

#[cfg(feature = "physics-bullet-native")]
fn static_physics_body_desc_with_nonzero_input_mass() -> MmdRuntimeFfiPhysicsRigidBodyDesc {
    MmdRuntimeFfiPhysicsRigidBodyDesc {
        shape: MmdRuntimeFfiPhysicsRigidBodyShape::Sphere as u32,
        shape_size: [0.5, 0.0, 0.0],
        position_xyz: [0.0, 10.0, 0.0],
        rotation_euler_xyz: [0.0; 3],
        mass: 1.0,
        linear_damping: 0.0,
        angular_damping: 0.0,
        friction: 0.5,
        restitution: 0.0,
        collision_group: 0,
        collision_mask: 0xffff,
        bone_index: 0,
        mode: MmdRuntimeFfiPhysicsRigidBodyMode::Static as u32,
        body_from_bone_position_xyz: [0.0; 3],
        body_from_bone_rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
        bone_from_body_position_xyz: [0.0; 3],
        bone_from_body_rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
    }
}

#[cfg(feature = "physics-bullet-native")]
#[test]
fn physics_world_descriptor_abi_keeps_dynamic_bone_solver_owned_on_forward_step() {
    assert_eq!(
        mmd_runtime_feature_flags() & FEATURE_PHYSICS_BULLET_NATIVE,
        FEATURE_PHYSICS_BULLET_NATIVE
    );

    let parents = [-1];
    let rest_positions = [0.0, 8.0, 0.0];
    let model = unsafe { mmd_runtime_model_create(parents.as_ptr(), rest_positions.as_ptr(), 1) };
    assert!(!model.is_null());
    let instance = unsafe { mmd_runtime_instance_create(model, 0) };
    assert!(!instance.is_null());
    assert!(unsafe { mmd_runtime_instance_evaluate_rest_pose(instance) });
    assert_eq!(
        unsafe {
            mmd_runtime_instance_set_physics_mode(instance, MmdRuntimeFfiPhysicsMode::Live as u32)
        },
        MmdRuntimeStatus::Ok
    );

    let bodies = [dynamic_bone_physics_body_desc()];
    let mut world = ptr::null_mut();
    assert_eq!(
        unsafe {
            mmd_runtime_physics_world_create(
                bodies.as_ptr(),
                bodies.len(),
                ptr::null(),
                0,
                &mut world,
            )
        },
        MmdRuntimeStatus::Ok
    );
    assert!(!world.is_null());

    let mut count = 0usize;
    assert_eq!(
        unsafe { mmd_runtime_physics_world_rigidbody_count(world, &mut count) },
        MmdRuntimeStatus::Ok
    );
    assert_eq!(count, 1);

    let mut seeded = 0usize;
    assert_eq!(
        unsafe { mmd_runtime_physics_world_reset(world, instance, &mut seeded) },
        MmdRuntimeStatus::Ok
    );
    assert_eq!(seeded, 1);

    let mut reset_states = [0.0f32; 7];
    assert_eq!(
        unsafe {
            mmd_runtime_physics_world_copy_rigidbody_states(
                world,
                reset_states.as_mut_ptr(),
                reset_states.len(),
            )
        },
        MmdRuntimeStatus::Ok
    );
    assert!(
        reset_states[1] < 8.0,
        "reset must include a solver settle: {reset_states:?}"
    );
    let mut reset_matrices = [0.0f32; 16];
    assert!(unsafe {
        mmd_runtime_instance_copy_world_matrices(
            instance,
            reset_matrices.as_mut_ptr(),
            reset_matrices.len(),
        )
    });
    assert_near(reset_matrices[13], reset_states[1], 1.0e-4);

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
        unsafe { mmd_runtime_physics_world_step_runtime(world, instance, 1.0 / 60.0, &mut report) },
        MmdRuntimeStatus::Ok
    );
    assert_eq!(report.tick.substeps, 2);
    assert_eq!(report.kinematic_rigidbodies_fed, 0);
    assert_eq!(report.bones_written_back, 1);

    let mut states = [0.0f32; 7];
    assert_eq!(
        unsafe {
            mmd_runtime_physics_world_copy_rigidbody_states(
                world,
                states.as_mut_ptr(),
                states.len(),
            )
        },
        MmdRuntimeStatus::Ok
    );
    assert!(states[1] < 8.0, "dynamic body should fall: {states:?}");

    let mut matrices = [0.0f32; 16];
    assert!(unsafe {
        mmd_runtime_instance_copy_world_matrices(instance, matrices.as_mut_ptr(), matrices.len())
    });
    assert!(matrices[13] < 8.0, "runtime bone should receive readback");

    assert!(unsafe { mmd_runtime_instance_evaluate_rest_pose(instance) });
    assert_eq!(
        unsafe { mmd_runtime_physics_world_reset(world, instance, &mut seeded) },
        MmdRuntimeStatus::Ok
    );
    assert_eq!(seeded, 1);
    let mut repeated_reset_states = [0.0f32; 7];
    assert_eq!(
        unsafe {
            mmd_runtime_physics_world_copy_rigidbody_states(
                world,
                repeated_reset_states.as_mut_ptr(),
                repeated_reset_states.len(),
            )
        },
        MmdRuntimeStatus::Ok
    );
    assert_slice_near(&repeated_reset_states, &reset_states, 1.0e-5);

    unsafe {
        mmd_runtime_physics_world_free(world);
        mmd_runtime_instance_free(instance);
        mmd_runtime_model_free(model);
    }
}

#[cfg(feature = "physics-bullet-native")]
#[test]
fn physics_world_descriptor_abi_static_mode_forces_zero_mass() {
    let parents = [-1];
    let rest_positions = [0.0, 10.0, 0.0];
    let model = unsafe { mmd_runtime_model_create(parents.as_ptr(), rest_positions.as_ptr(), 1) };
    assert!(!model.is_null());
    let instance = unsafe { mmd_runtime_instance_create(model, 0) };
    assert!(!instance.is_null());
    assert!(unsafe { mmd_runtime_instance_evaluate_rest_pose(instance) });
    assert_eq!(
        unsafe {
            mmd_runtime_instance_set_physics_mode(instance, MmdRuntimeFfiPhysicsMode::Live as u32)
        },
        MmdRuntimeStatus::Ok
    );

    let bodies = [static_physics_body_desc_with_nonzero_input_mass()];
    let mut world = ptr::null_mut();
    assert_eq!(
        unsafe {
            mmd_runtime_physics_world_create(
                bodies.as_ptr(),
                bodies.len(),
                ptr::null(),
                0,
                &mut world,
            )
        },
        MmdRuntimeStatus::Ok
    );
    assert!(!world.is_null());

    assert_eq!(
        unsafe { mmd_runtime_physics_world_reset(world, instance, ptr::null_mut()) },
        MmdRuntimeStatus::Ok
    );
    assert_eq!(
        unsafe {
            mmd_runtime_physics_world_step_runtime(world, instance, 1.0 / 60.0, ptr::null_mut())
        },
        MmdRuntimeStatus::Ok
    );
    let mut states = [0.0f32; 7];
    assert_eq!(
        unsafe {
            mmd_runtime_physics_world_copy_rigidbody_states(
                world,
                states.as_mut_ptr(),
                states.len(),
            )
        },
        MmdRuntimeStatus::Ok
    );
    assert_near(states[1], 10.0, 1.0e-4);

    unsafe {
        mmd_runtime_physics_world_free(world);
        mmd_runtime_instance_free(instance);
        mmd_runtime_model_free(model);
    }
}

#[cfg(feature = "physics-bullet-native")]
#[test]
fn physics_world_copy_rigidbody_bindings_and_physics_driven_bone_mask() {
    let static_body = MmdRuntimeFfiPhysicsRigidBodyDesc {
        bone_index: 0,
        ..static_physics_body_desc_with_nonzero_input_mass()
    };
    let dynamic_body = MmdRuntimeFfiPhysicsRigidBodyDesc {
        bone_index: 1,
        ..dynamic_physics_body_desc()
    };
    let unbound_body = MmdRuntimeFfiPhysicsRigidBodyDesc {
        bone_index: -1,
        ..dynamic_bone_physics_body_desc()
    };
    let bodies = [static_body, dynamic_body, unbound_body];
    let mut world = ptr::null_mut();
    assert_eq!(
        unsafe {
            mmd_runtime_physics_world_create(
                bodies.as_ptr(),
                bodies.len(),
                ptr::null(),
                0,
                &mut world,
            )
        },
        MmdRuntimeStatus::Ok
    );
    assert!(!world.is_null());

    let mut count = 0usize;
    let mut bindings = [MmdRuntimeFfiPhysicsRigidBodyBinding {
        bone_index: -2,
        mode: u32::MAX,
    }; 3];
    assert_eq!(
        unsafe {
            mmd_runtime_physics_world_copy_rigidbody_bindings(
                world,
                bindings.as_mut_ptr(),
                bindings.len(),
                &mut count,
            )
        },
        MmdRuntimeStatus::Ok
    );
    assert_eq!(count, 3);
    assert_eq!(
        bindings[0],
        MmdRuntimeFfiPhysicsRigidBodyBinding {
            bone_index: 0,
            mode: MmdRuntimeFfiPhysicsRigidBodyMode::Static as u32,
        }
    );
    assert_eq!(
        bindings[1],
        MmdRuntimeFfiPhysicsRigidBodyBinding {
            bone_index: 1,
            mode: MmdRuntimeFfiPhysicsRigidBodyMode::Dynamic as u32,
        }
    );
    assert_eq!(
        bindings[2],
        MmdRuntimeFfiPhysicsRigidBodyBinding {
            bone_index: -1,
            mode: MmdRuntimeFfiPhysicsRigidBodyMode::DynamicBone as u32,
        }
    );

    let mut too_small = [MmdRuntimeFfiPhysicsRigidBodyBinding {
        bone_index: -2,
        mode: u32::MAX,
    }; 1];
    let mut small_count = 0usize;
    assert_eq!(
        unsafe {
            mmd_runtime_physics_world_copy_rigidbody_bindings(
                world,
                too_small.as_mut_ptr(),
                too_small.len(),
                &mut small_count,
            )
        },
        MmdRuntimeStatus::BufferTooSmall
    );
    assert_eq!(small_count, 3);

    let mut mask = [0xffu8; 2];
    assert_eq!(
        unsafe {
            mmd_runtime_physics_world_physics_driven_bone_mask(world, mask.as_mut_ptr(), mask.len())
        },
        MmdRuntimeStatus::Ok
    );
    assert_eq!(mask, [0u8, 1u8]);

    unsafe {
        mmd_runtime_physics_world_free(world);
    }
}

#[cfg(feature = "physics-bullet-native")]
#[test]
fn physics_world_descriptor_abi_rejects_invalid_inputs() {
    let mut world = ptr::null_mut();
    let mut body = dynamic_physics_body_desc();
    body.shape = 99;
    assert_eq!(
        unsafe { mmd_runtime_physics_world_create(&body, 1, ptr::null(), 0, &mut world) },
        MmdRuntimeStatus::InvalidInput
    );
    assert!(world.is_null());

    body = dynamic_physics_body_desc();
    body.position_xyz[0] = f32::NAN;
    assert_eq!(
        unsafe { mmd_runtime_physics_world_create(&body, 1, ptr::null(), 0, &mut world) },
        MmdRuntimeStatus::InvalidInput
    );
    assert!(world.is_null());

    body = dynamic_physics_body_desc();
    assert_eq!(
        unsafe { mmd_runtime_physics_world_create(&body, 1, ptr::null(), 0, ptr::null_mut()) },
        MmdRuntimeStatus::InvalidInput
    );
}

#[cfg(feature = "physics-bullet-native")]
fn create_physics_bake_clip() -> *mut MmdRuntimeClip {
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
            frame: 30,
            position_xyz: [0.0, 1.0, 0.0],
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
            frame: 30,
            weight: 1.0,
        },
    ];
    unsafe {
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
    }
}

#[cfg(feature = "physics-bullet-native")]
fn create_bone_only_physics_bake_clip() -> *mut MmdRuntimeClip {
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
            frame: 30,
            position_xyz: [0.0, 1.0, 0.0],
            rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
        },
    ];
    unsafe {
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
    }
}

#[cfg(feature = "physics-bullet-native")]
#[test]
fn physics_world_bake_clip_frames_matches_manual_sequential_loop() {
    let parents = [-1];
    let rest_positions = [0.0, 8.0, 0.0];
    let model = unsafe { mmd_runtime_model_create(parents.as_ptr(), rest_positions.as_ptr(), 1) };
    assert!(!model.is_null());
    let manual = unsafe { mmd_runtime_instance_create(model, 1) };
    let baked = unsafe { mmd_runtime_instance_create(model, 1) };
    assert!(!manual.is_null());
    assert!(!baked.is_null());
    assert!(unsafe { mmd_runtime_instance_evaluate_rest_pose(manual) });
    assert!(unsafe { mmd_runtime_instance_evaluate_rest_pose(baked) });
    assert_eq!(
        unsafe {
            mmd_runtime_instance_set_physics_mode(manual, MmdRuntimeFfiPhysicsMode::Live as u32)
        },
        MmdRuntimeStatus::Ok
    );
    assert_eq!(
        unsafe {
            mmd_runtime_instance_set_physics_mode(baked, MmdRuntimeFfiPhysicsMode::Live as u32)
        },
        MmdRuntimeStatus::Ok
    );
    let clip = create_physics_bake_clip();
    assert!(!clip.is_null());

    let bodies = [dynamic_bone_physics_body_desc()];
    let mut manual_world = ptr::null_mut();
    let mut bake_world = ptr::null_mut();
    assert_eq!(
        unsafe {
            mmd_runtime_physics_world_create(
                bodies.as_ptr(),
                bodies.len(),
                ptr::null(),
                0,
                &mut manual_world,
            )
        },
        MmdRuntimeStatus::Ok
    );
    assert_eq!(
        unsafe {
            mmd_runtime_physics_world_create(
                bodies.as_ptr(),
                bodies.len(),
                ptr::null(),
                0,
                &mut bake_world,
            )
        },
        MmdRuntimeStatus::Ok
    );
    assert_eq!(
        unsafe { mmd_runtime_physics_world_reset(manual_world, manual, ptr::null_mut()) },
        MmdRuntimeStatus::Ok
    );
    assert_eq!(
        unsafe { mmd_runtime_physics_world_reset(bake_world, baked, ptr::null_mut()) },
        MmdRuntimeStatus::Ok
    );

    let mut manual_world_out = [0.0f32; 48];
    let mut manual_morphs = [0.0f32; 3];
    let mut manual_report = MmdRuntimeFfiPhysicsWorldStepReport {
        tick: MmdRuntimeFfiPhysicsStepStats {
            input_dt_seconds: 0.0,
            clamped_dt_seconds: 0.0,
            substeps: 0,
            accumulator_seconds: 0.0,
        },
        kinematic_rigidbodies_fed: 0,
        bones_written_back: 0,
    };
    // Matches bake seed-only-first: sample 0 uses the shared offline-bake
    // initializer without stepping; later samples step through the public API.
    for frame_index in 0..3 {
        assert_eq!(
            unsafe {
                mmd_runtime_instance_evaluate_clip_frame_before_physics(
                    manual,
                    clip,
                    frame_index as f32 * 15.0,
                )
            },
            MmdRuntimeStatus::Ok
        );
        if frame_index == 0 {
            use mmd_anim_physics_bullet::RuntimePhysicsBridgeExt;

            let manual_world = unsafe { &mut *manual_world };
            let manual = unsafe { &mut *manual };
            manual_world
                .world
                .initialize_runtime_physics_bake(&mut manual.runtime)
                .unwrap();
            manual_world.next_bake_sample_is_seed_only = false;
            manual.refresh_matrix_caches();
        } else {
            assert_eq!(
                unsafe {
                    mmd_runtime_physics_world_step_runtime(
                        manual_world,
                        manual,
                        1.0 / 60.0,
                        &mut manual_report,
                    )
                },
                MmdRuntimeStatus::Ok
            );
        }
        let world_start = frame_index * 16;
        assert!(unsafe {
            mmd_runtime_instance_copy_world_matrices(
                manual,
                manual_world_out[world_start..].as_mut_ptr(),
                16,
            )
        });
        assert!(unsafe {
            mmd_runtime_instance_copy_morph_weights(
                manual,
                manual_morphs[frame_index..].as_mut_ptr(),
                1,
            )
        });
    }

    let mut baked_world_out = [0.0f32; 48];
    let mut baked_morphs = [0.0f32; 3];
    let mut baked_report = MmdRuntimeFfiPhysicsWorldStepReport {
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
            mmd_runtime_physics_world_bake_clip_frames(
                bake_world,
                baked,
                clip,
                0.0,
                15.0,
                1.0 / 60.0,
                3,
                baked_world_out.as_mut_ptr(),
                baked_world_out.len(),
                baked_morphs.as_mut_ptr(),
                baked_morphs.len(),
                &mut baked_report,
            )
        },
        MmdRuntimeStatus::Ok
    );

    assert_slice_near(&baked_world_out, &manual_world_out, 1.0e-5);
    assert_slice_near(&baked_morphs, &manual_morphs, 0.0);
    // Multi-sample bake: last report is the final actual physics step (not seed-only).
    assert_eq!(baked_report.tick.substeps, manual_report.tick.substeps);
    assert_eq!(
        baked_report.bones_written_back,
        manual_report.bones_written_back
    );
    assert_eq!(manual_report.kinematic_rigidbodies_fed, 0);
    assert_eq!(baked_report.kinematic_rigidbodies_fed, 0);
    assert_eq!(baked_report.bones_written_back, 1);

    assert_eq!(
        unsafe {
            mmd_runtime_physics_world_bake_clip_frames(
                bake_world,
                baked,
                clip,
                0.0,
                15.0,
                f32::NAN,
                3,
                baked_world_out.as_mut_ptr(),
                baked_world_out.len(),
                baked_morphs.as_mut_ptr(),
                baked_morphs.len(),
                ptr::null_mut(),
            )
        },
        MmdRuntimeStatus::InvalidInput
    );
    assert_eq!(
        unsafe {
            mmd_runtime_physics_world_bake_clip_frames(
                bake_world,
                baked,
                clip,
                0.0,
                15.0,
                1.0 / 60.0,
                3,
                baked_world_out.as_mut_ptr(),
                baked_world_out.len() - 1,
                baked_morphs.as_mut_ptr(),
                baked_morphs.len(),
                ptr::null_mut(),
            )
        },
        MmdRuntimeStatus::BufferTooSmall
    );

    unsafe {
        mmd_runtime_physics_world_free(bake_world);
        mmd_runtime_physics_world_free(manual_world);
        mmd_runtime_clip_free(clip);
        mmd_runtime_instance_free(baked);
        mmd_runtime_instance_free(manual);
        mmd_runtime_model_free(model);
    }
}

#[cfg(feature = "physics-bullet-native")]
fn zero_physics_step_report() -> MmdRuntimeFfiPhysicsWorldStepReport {
    MmdRuntimeFfiPhysicsWorldStepReport {
        tick: MmdRuntimeFfiPhysicsStepStats {
            input_dt_seconds: 0.0,
            clamped_dt_seconds: 0.0,
            substeps: 0,
            accumulator_seconds: 0.0,
        },
        kinematic_rigidbodies_fed: 0,
        bones_written_back: 0,
    }
}

#[cfg(feature = "physics-bullet-native")]
fn assert_zero_physics_step_report(report: &MmdRuntimeFfiPhysicsWorldStepReport) {
    assert_eq!(report.tick.substeps, 0);
    assert_eq!(report.tick.input_dt_seconds, 0.0);
    assert_eq!(report.tick.clamped_dt_seconds, 0.0);
    assert_eq!(report.tick.accumulator_seconds, 0.0);
    assert_eq!(report.kinematic_rigidbodies_fed, 0);
    assert_eq!(report.bones_written_back, 0);
}

#[cfg(feature = "physics-bullet-native")]
#[test]
fn physics_world_bake_clip_frames_seed_only_state_contract() {
    let parents = [-1];
    let rest_positions = [0.0, 8.0, 0.0];
    let model = unsafe { mmd_runtime_model_create(parents.as_ptr(), rest_positions.as_ptr(), 1) };
    assert!(!model.is_null());
    let instance = unsafe { mmd_runtime_instance_create(model, 1) };
    assert!(!instance.is_null());
    assert!(unsafe { mmd_runtime_instance_evaluate_rest_pose(instance) });
    assert_eq!(
        unsafe {
            mmd_runtime_instance_set_physics_mode(instance, MmdRuntimeFfiPhysicsMode::Live as u32)
        },
        MmdRuntimeStatus::Ok
    );
    let clip = create_physics_bake_clip();
    assert!(!clip.is_null());

    let bodies = [dynamic_physics_body_desc()];
    let mut world = ptr::null_mut();
    assert_eq!(
        unsafe {
            mmd_runtime_physics_world_create(
                bodies.as_ptr(),
                bodies.len(),
                ptr::null(),
                0,
                &mut world,
            )
        },
        MmdRuntimeStatus::Ok
    );
    assert!(!world.is_null());

    // Creation arms seed-only; frame_count == 0 must not consume it.
    let mut report = zero_physics_step_report();
    let mut world_out = [0.0f32; 16];
    let mut morphs = [0.0f32; 1];
    assert_eq!(
        unsafe {
            mmd_runtime_physics_world_bake_clip_frames(
                world,
                instance,
                clip,
                0.0,
                15.0,
                1.0 / 60.0,
                0,
                world_out.as_mut_ptr(),
                0,
                morphs.as_mut_ptr(),
                0,
                &mut report,
            )
        },
        MmdRuntimeStatus::Ok
    );
    assert_zero_physics_step_report(&report);

    // First real sample after create is seed-only: zero forward-step report and
    // the evaluated pose copied exactly, with no reset settle.
    report = zero_physics_step_report();
    assert_eq!(
        unsafe {
            mmd_runtime_physics_world_bake_clip_frames(
                world,
                instance,
                clip,
                0.0,
                15.0,
                1.0 / 60.0,
                1,
                world_out.as_mut_ptr(),
                world_out.len(),
                morphs.as_mut_ptr(),
                morphs.len(),
                &mut report,
            )
        },
        MmdRuntimeStatus::Ok
    );
    assert_zero_physics_step_report(&report);
    let mut seed_only_states = [0.0f32; 7];
    assert_eq!(
        unsafe {
            mmd_runtime_physics_world_copy_rigidbody_states(
                world,
                seed_only_states.as_mut_ptr(),
                seed_only_states.len(),
            )
        },
        MmdRuntimeStatus::Ok
    );
    assert_near(world_out[13], seed_only_states[1], 1.0e-4);
    assert_near(seed_only_states[1], 8.0, 1.0e-4);
    let seed_only_y = world_out[13];

    // Continuation chunk without reset: first sample steps normally.
    report = zero_physics_step_report();
    let mut cont_out = [0.0f32; 16];
    let mut cont_morphs = [0.0f32; 1];
    assert_eq!(
        unsafe {
            mmd_runtime_physics_world_bake_clip_frames(
                world,
                instance,
                clip,
                15.0,
                15.0,
                1.0 / 60.0,
                1,
                cont_out.as_mut_ptr(),
                cont_out.len(),
                cont_morphs.as_mut_ptr(),
                cont_morphs.len(),
                &mut report,
            )
        },
        MmdRuntimeStatus::Ok
    );
    assert!(
        report.tick.substeps > 0 || report.bones_written_back > 0,
        "continuation first sample must step physics: {report:?}"
    );

    // Successful reset re-arms seed-only.
    assert_eq!(
        unsafe { mmd_runtime_physics_world_reset(world, instance, ptr::null_mut()) },
        MmdRuntimeStatus::Ok
    );
    report = zero_physics_step_report();
    assert_eq!(
        unsafe {
            mmd_runtime_physics_world_bake_clip_frames(
                world,
                instance,
                clip,
                0.0,
                15.0,
                1.0 / 60.0,
                1,
                world_out.as_mut_ptr(),
                world_out.len(),
                morphs.as_mut_ptr(),
                morphs.len(),
                &mut report,
            )
        },
        MmdRuntimeStatus::Ok
    );
    assert_zero_physics_step_report(&report);
    assert_near(world_out[13], seed_only_y, 1.0e-4);

    // Multi-sample after re-arm: first seed-only, second steps; last report is the step.
    assert_eq!(
        unsafe { mmd_runtime_physics_world_reset(world, instance, ptr::null_mut()) },
        MmdRuntimeStatus::Ok
    );
    report = zero_physics_step_report();
    let mut multi_out = [0.0f32; 32];
    let mut multi_morphs = [0.0f32; 2];
    assert_eq!(
        unsafe {
            mmd_runtime_physics_world_bake_clip_frames(
                world,
                instance,
                clip,
                0.0,
                15.0,
                1.0 / 60.0,
                2,
                multi_out.as_mut_ptr(),
                multi_out.len(),
                multi_morphs.as_mut_ptr(),
                multi_morphs.len(),
                &mut report,
            )
        },
        MmdRuntimeStatus::Ok
    );
    assert_eq!(report.bones_written_back, 1);
    assert!(report.tick.substeps > 0);
    // Seed-only first sample exposes the deterministic evaluated pose.
    assert_near(multi_out[13], seed_only_y, 1.0e-4);

    // Explicit step_runtime disarms seed-only so the next bake sample steps.
    assert_eq!(
        unsafe { mmd_runtime_physics_world_reset(world, instance, ptr::null_mut()) },
        MmdRuntimeStatus::Ok
    );
    report = zero_physics_step_report();
    assert_eq!(
        unsafe { mmd_runtime_physics_world_step_runtime(world, instance, 1.0 / 60.0, &mut report) },
        MmdRuntimeStatus::Ok
    );
    assert!(report.tick.substeps > 0 || report.bones_written_back > 0);
    report = zero_physics_step_report();
    assert_eq!(
        unsafe {
            mmd_runtime_physics_world_bake_clip_frames(
                world,
                instance,
                clip,
                0.0,
                15.0,
                1.0 / 60.0,
                1,
                world_out.as_mut_ptr(),
                world_out.len(),
                morphs.as_mut_ptr(),
                morphs.len(),
                &mut report,
            )
        },
        MmdRuntimeStatus::Ok
    );
    assert!(
        report.tick.substeps > 0 || report.bones_written_back > 0,
        "after explicit step_runtime, bake first sample must not be seed-only: {report:?}"
    );

    // Existing invalid-input / buffer behavior still holds after stateful changes.
    assert_eq!(
        unsafe {
            mmd_runtime_physics_world_bake_clip_frames(
                world,
                instance,
                clip,
                0.0,
                15.0,
                f32::NAN,
                1,
                world_out.as_mut_ptr(),
                world_out.len(),
                morphs.as_mut_ptr(),
                morphs.len(),
                ptr::null_mut(),
            )
        },
        MmdRuntimeStatus::InvalidInput
    );
    assert_eq!(
        unsafe {
            mmd_runtime_physics_world_bake_clip_frames(
                world,
                instance,
                clip,
                0.0,
                15.0,
                1.0 / 60.0,
                1,
                world_out.as_mut_ptr(),
                world_out.len() - 1,
                morphs.as_mut_ptr(),
                morphs.len(),
                ptr::null_mut(),
            )
        },
        MmdRuntimeStatus::BufferTooSmall
    );

    unsafe {
        mmd_runtime_physics_world_free(world);
        mmd_runtime_clip_free(clip);
        mmd_runtime_instance_free(instance);
        mmd_runtime_model_free(model);
    }
}

#[test]
fn split_physics_clip_evaluation_matches_full_evaluation_without_external_physics() {
    let parents = [-1];
    let rest_positions = [0.0, 0.0, 0.0];
    let model = unsafe { mmd_runtime_model_create(parents.as_ptr(), rest_positions.as_ptr(), 1) };
    assert!(!model.is_null());
    let full = unsafe { mmd_runtime_instance_create(model, 0) };
    let split = unsafe { mmd_runtime_instance_create(model, 0) };
    assert!(!full.is_null());
    assert!(!split.is_null());

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

    assert!(unsafe { mmd_runtime_instance_evaluate_clip_frame(full, clip, 30.0) });
    assert_eq!(
        unsafe { mmd_runtime_instance_evaluate_clip_frame_before_physics(split, clip, 30.0) },
        MmdRuntimeStatus::Ok
    );
    assert_eq!(
        unsafe {
            mmd_runtime_instance_evaluate_current_pose_after_physics_with_ik_options(split, 0.0, 4)
        },
        MmdRuntimeStatus::Ok
    );

    let mut full_world = [0.0f32; 16];
    let mut split_world = [0.0f32; 16];
    assert!(unsafe {
        mmd_runtime_instance_copy_world_matrices(full, full_world.as_mut_ptr(), full_world.len())
    });
    assert!(unsafe {
        mmd_runtime_instance_copy_world_matrices(split, split_world.as_mut_ptr(), split_world.len())
    });
    assert_slice_near(&split_world, &full_world, 1.0e-6);

    assert_eq!(
        unsafe {
            mmd_runtime_instance_evaluate_clip_frame_before_physics_with_ik_options(
                split,
                clip,
                30.0,
                f32::NAN,
                0,
            )
        },
        MmdRuntimeStatus::InvalidInput
    );
    assert_eq!(
        unsafe {
            mmd_runtime_instance_evaluate_current_pose_after_physics_with_ik_options(
                split,
                f32::NAN,
                0,
            )
        },
        MmdRuntimeStatus::InvalidInput
    );

    unsafe {
        mmd_runtime_clip_free(clip);
        mmd_runtime_instance_free(split);
        mmd_runtime_instance_free(full);
        mmd_runtime_model_free(model);
    }
}

#[test]
fn apply_physics_world_matrices_updates_selected_bones_and_caches() {
    let parents = [-1, 0];
    let rest_positions = [0.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    let model = unsafe { mmd_runtime_model_create(parents.as_ptr(), rest_positions.as_ptr(), 2) };
    assert!(!model.is_null());
    let instance = unsafe { mmd_runtime_instance_create(model, 0) };
    assert!(!instance.is_null());
    assert!(unsafe { mmd_runtime_instance_evaluate_rest_pose(instance) });

    let root = glam::Mat4::IDENTITY.to_cols_array();
    let child = glam::Mat4::from_translation(glam::Vec3::new(0.0, 5.0, 0.0)).to_cols_array();
    let mut physics_world = [0.0f32; 32];
    physics_world[0..16].copy_from_slice(&root);
    physics_world[16..32].copy_from_slice(&child);
    let mask = [0u8, 1u8];
    let mut updated = 0usize;
    assert_eq!(
        unsafe {
            mmd_runtime_instance_apply_physics_world_matrices(
                instance,
                physics_world.as_ptr(),
                physics_world.len(),
                mask.as_ptr(),
                mask.len(),
                &mut updated,
            )
        },
        MmdRuntimeStatus::Ok
    );
    assert_eq!(updated, 1);

    let mut matrices = [0.0f32; 32];
    assert!(unsafe {
        mmd_runtime_instance_copy_world_matrices(instance, matrices.as_mut_ptr(), matrices.len())
    });
    assert_near(matrices[16 + 13], 5.0, 1.0e-5);
    let direct = unsafe { mmd_runtime_instance_world_matrices(instance) };
    assert!(!direct.is_null());
    let direct = unsafe { slice::from_raw_parts(direct, 32) };
    assert_near(direct[16 + 13], 5.0, 1.0e-5);

    physics_world[0] = f32::NAN;
    let moved_child = glam::Mat4::from_translation(glam::Vec3::new(0.0, 6.0, 0.0)).to_cols_array();
    physics_world[16..32].copy_from_slice(&moved_child);
    updated = 0;
    assert_eq!(
        unsafe {
            mmd_runtime_instance_apply_physics_world_matrices(
                instance,
                physics_world.as_ptr(),
                physics_world.len(),
                mask.as_ptr(),
                mask.len(),
                &mut updated,
            )
        },
        MmdRuntimeStatus::Ok
    );
    assert_eq!(updated, 1);

    assert_eq!(
        unsafe {
            mmd_runtime_instance_apply_physics_world_matrices(
                instance,
                physics_world.as_ptr(),
                physics_world.len() - 1,
                mask.as_ptr(),
                mask.len(),
                ptr::null_mut(),
            )
        },
        MmdRuntimeStatus::BufferTooSmall
    );
    assert_eq!(
        unsafe {
            mmd_runtime_instance_apply_physics_world_matrices(
                instance,
                physics_world.as_ptr(),
                physics_world.len(),
                mask.as_ptr(),
                mask.len() - 1,
                ptr::null_mut(),
            )
        },
        MmdRuntimeStatus::BufferTooSmall
    );

    unsafe {
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
fn descriptor_v1_creates_owned_model_with_metadata() {
    let model = {
        let mut bones = vec![
            MmdRuntimeModelBoneDescriptor {
                parent_index: -1,
                rest_position_xyz: [1.0, 2.0, 3.0],
                transform_order: 4,
                flags: MODEL_BONE_FLAG_FIXED_AXIS | MODEL_BONE_FLAG_LOCAL_AXIS,
                fixed_axis_xyz: [0.0, 2.0, 0.0],
                local_axis_x_xyz: [0.0, 1.0, 0.0],
                local_axis_z_xyz: [0.0, 0.0, 1.0],
            },
            MmdRuntimeModelBoneDescriptor {
                parent_index: 0,
                rest_position_xyz: [5.0, 2.0, 3.0],
                transform_order: 7,
                flags: MODEL_BONE_FLAG_TRANSFORM_AFTER_PHYSICS,
                fixed_axis_xyz: [0.0; 3],
                local_axis_x_xyz: [0.0; 3],
                local_axis_z_xyz: [0.0; 3],
            },
        ];
        let mut links = vec![MmdRuntimeModelIkLinkDescriptor {
            bone_index: 0,
            flags: IK_LINK_FLAG_ANGLE_LIMIT,
            angle_limit_min_xyz: [-1.0; 3],
            angle_limit_max_xyz: [1.0; 3],
        }];
        let mut solvers = vec![MmdRuntimeModelIkSolverDescriptor {
            ik_bone_index: 1,
            target_bone_index: 0,
            link_offset: 0,
            link_count: 1,
            iteration_count: 1,
            limit_angle: 0.5,
        }];
        let mut appends = vec![MmdRuntimeModelAppendDescriptor {
            target_bone_index: 1,
            source_bone_index: 0,
            ratio: 0.25,
            flags: APPEND_FLAG_ROTATION | APPEND_FLAG_LOCAL,
        }];
        let mut bone_offsets = vec![MmdRuntimeModelBoneMorphOffsetDescriptor {
            morph_index: 0,
            target_bone_index: 1,
            position_offset_xyz: [0.1, 0.0, 0.0],
            rotation_offset_xyzw: [0.0, 0.0, 0.0, 1.0],
        }];
        let mut group_offsets = vec![MmdRuntimeModelGroupMorphOffsetDescriptor {
            morph_index: 1,
            child_morph_index: 0,
            ratio: 0.5,
        }];
        let mut descriptor = MmdRuntimeModelDescriptor {
            struct_size: std::mem::size_of::<MmdRuntimeModelDescriptor>() as u32,
            descriptor_version: MMD_RUNTIME_MODEL_DESCRIPTOR_VERSION_V1,
            flags: 0,
            reserved: 0,
            bones: bones.as_ptr(),
            bone_count: bones.len(),
            ik_solvers: solvers.as_ptr(),
            ik_solver_count: solvers.len(),
            ik_links: links.as_ptr(),
            ik_link_count: links.len(),
            append_transforms: appends.as_ptr(),
            append_transform_count: appends.len(),
            morph_count: 2,
            bone_morph_offsets: bone_offsets.as_ptr(),
            bone_morph_offset_count: bone_offsets.len(),
            group_morph_offsets: group_offsets.as_ptr(),
            group_morph_offset_count: group_offsets.len(),
        };
        let model = unsafe { mmd_runtime_model_create_from_descriptor(&descriptor) };
        assert!(!model.is_null());

        bones.fill(MmdRuntimeModelBoneDescriptor {
            parent_index: -1,
            rest_position_xyz: [99.0; 3],
            transform_order: -99,
            flags: 0,
            fixed_axis_xyz: [0.0; 3],
            local_axis_x_xyz: [0.0; 3],
            local_axis_z_xyz: [0.0; 3],
        });
        links.fill(MmdRuntimeModelIkLinkDescriptor {
            bone_index: u32::MAX,
            flags: 0,
            angle_limit_min_xyz: [99.0; 3],
            angle_limit_max_xyz: [99.0; 3],
        });
        solvers.fill(MmdRuntimeModelIkSolverDescriptor {
            ik_bone_index: u32::MAX,
            target_bone_index: u32::MAX,
            link_offset: 0,
            link_count: 0,
            iteration_count: 0,
            limit_angle: 99.0,
        });
        appends.fill(MmdRuntimeModelAppendDescriptor {
            target_bone_index: u32::MAX,
            source_bone_index: u32::MAX,
            ratio: 99.0,
            flags: 0,
        });
        bone_offsets.fill(MmdRuntimeModelBoneMorphOffsetDescriptor {
            morph_index: u32::MAX,
            target_bone_index: u32::MAX,
            position_offset_xyz: [99.0; 3],
            rotation_offset_xyzw: [99.0; 4],
        });
        group_offsets.fill(MmdRuntimeModelGroupMorphOffsetDescriptor {
            morph_index: u32::MAX,
            child_morph_index: u32::MAX,
            ratio: 99.0,
        });
        descriptor.struct_size = 0;
        descriptor.descriptor_version = 0;
        descriptor.bones = ptr::null();
        descriptor.bone_count = 0;
        descriptor.ik_solvers = ptr::null();
        descriptor.ik_solver_count = 0;
        descriptor.ik_links = ptr::null();
        descriptor.ik_link_count = 0;
        descriptor.append_transforms = ptr::null();
        descriptor.append_transform_count = 0;
        descriptor.morph_count = 0;
        descriptor.bone_morph_offsets = ptr::null();
        descriptor.bone_morph_offset_count = 0;
        descriptor.group_morph_offsets = ptr::null();
        descriptor.group_morph_offset_count = 0;
        std::hint::black_box((
            &bones,
            &links,
            &solvers,
            &appends,
            &bone_offsets,
            &group_offsets,
            &descriptor,
        ));
        model
    };
    let arena = unsafe { &(*model).model };
    assert_eq!(
        arena.rest_position(BoneIndex(0)),
        glam::Vec3A::new(1.0, 2.0, 3.0)
    );
    assert_eq!(
        arena.rest_position(BoneIndex(1)),
        glam::Vec3A::new(4.0, 0.0, 0.0)
    );
    assert_eq!(arena.transform_order(BoneIndex(0)), 4);
    assert_eq!(arena.transform_order(BoneIndex(1)), 7);
    assert!(arena.transform_after_physics(BoneIndex(1)));
    assert_eq!(arena.fixed_axis(BoneIndex(0)), Some(glam::Vec3A::Y));
    assert!(arena.local_axis(BoneIndex(0)).is_some());
    assert_eq!(arena.ik_solvers().len(), 1);
    assert_eq!(arena.append_transforms().len(), 1);
    assert_eq!(arena.morph_count(), 2);

    let instance = unsafe { mmd_runtime_instance_create(model, 0) };
    assert!(!instance.is_null());
    assert!(unsafe { mmd_runtime_instance_evaluate_rest_pose(instance) });
    let mut world = [0.0f32; 32];
    assert!(unsafe {
        mmd_runtime_instance_copy_world_matrices(instance, world.as_mut_ptr(), world.len())
    });
    assert_eq!(&world[12..15], &[1.0, 2.0, 3.0]);
    assert_eq!(&world[28..31], &[5.0, 2.0, 3.0]);
    let mut skinning = [0.0f32; 32];
    assert!(unsafe {
        mmd_runtime_instance_copy_skinning_matrices(instance, skinning.as_mut_ptr(), skinning.len())
    });
    assert_eq!(
        &skinning[..16],
        glam::Mat4::IDENTITY.to_cols_array().as_slice()
    );
    assert_eq!(
        &skinning[16..],
        glam::Mat4::IDENTITY.to_cols_array().as_slice()
    );

    unsafe {
        mmd_runtime_instance_free(instance);
        mmd_runtime_model_free(model);
    }
}

#[test]
fn descriptor_v1_rejects_invalid_parent_index_with_indexed_error() {
    let bones = [MmdRuntimeModelBoneDescriptor {
        parent_index: 1,
        rest_position_xyz: [0.0; 3],
        transform_order: 0,
        flags: 0,
        fixed_axis_xyz: [0.0; 3],
        local_axis_x_xyz: [0.0; 3],
        local_axis_z_xyz: [0.0; 3],
    }];
    let descriptor = MmdRuntimeModelDescriptor {
        struct_size: std::mem::size_of::<MmdRuntimeModelDescriptor>() as u32,
        descriptor_version: MMD_RUNTIME_MODEL_DESCRIPTOR_VERSION_V1,
        flags: 0,
        reserved: 0,
        bones: bones.as_ptr(),
        bone_count: bones.len(),
        ik_solvers: ptr::null(),
        ik_solver_count: 0,
        ik_links: ptr::null(),
        ik_link_count: 0,
        append_transforms: ptr::null(),
        append_transform_count: 0,
        morph_count: 0,
        bone_morph_offsets: ptr::null(),
        bone_morph_offset_count: 0,
        group_morph_offsets: ptr::null(),
        group_morph_offset_count: 0,
    };
    let model = unsafe { mmd_runtime_model_create_from_descriptor(&descriptor) };
    assert!(model.is_null());
    assert!(
        last_error_cstr()
            .expect("indexed descriptor error")
            .to_string_lossy()
            .contains("bones[0].parent")
    );
}

fn descriptor_v1_header_fixture() -> (
    Vec<MmdRuntimeModelBoneDescriptor>,
    MmdRuntimeModelDescriptor,
) {
    let bones = vec![MmdRuntimeModelBoneDescriptor {
        parent_index: -1,
        rest_position_xyz: [0.0; 3],
        transform_order: 0,
        flags: 0,
        fixed_axis_xyz: [0.0; 3],
        local_axis_x_xyz: [0.0; 3],
        local_axis_z_xyz: [0.0; 3],
    }];
    let descriptor = MmdRuntimeModelDescriptor {
        struct_size: std::mem::size_of::<MmdRuntimeModelDescriptor>() as u32,
        descriptor_version: MMD_RUNTIME_MODEL_DESCRIPTOR_VERSION_V1,
        flags: 0,
        reserved: 0,
        bones: bones.as_ptr(),
        bone_count: bones.len(),
        ik_solvers: ptr::null(),
        ik_solver_count: 0,
        ik_links: ptr::null(),
        ik_link_count: 0,
        append_transforms: ptr::null(),
        append_transform_count: 0,
        morph_count: 0,
        bone_morph_offsets: ptr::null(),
        bone_morph_offset_count: 0,
        group_morph_offsets: ptr::null(),
        group_morph_offset_count: 0,
    };
    (bones, descriptor)
}

#[test]
fn descriptor_v1_rejects_header_and_pointer_contracts() {
    let (bones, mut descriptor) = descriptor_v1_header_fixture();

    descriptor.struct_size = 0;
    assert!(unsafe { mmd_runtime_model_create_from_descriptor(&descriptor) }.is_null());
    assert!(
        last_error_cstr()
            .expect("struct size error")
            .to_string_lossy()
            .contains("descriptor.struct_size")
    );

    descriptor.struct_size = std::mem::size_of::<MmdRuntimeModelDescriptor>() as u32;
    descriptor.descriptor_version = 99;
    assert!(unsafe { mmd_runtime_model_create_from_descriptor(&descriptor) }.is_null());
    assert!(
        last_error_cstr()
            .expect("version error")
            .to_string_lossy()
            .contains("descriptor.descriptor_version")
    );

    descriptor.descriptor_version = MMD_RUNTIME_MODEL_DESCRIPTOR_VERSION_V1;
    descriptor.flags = 1;
    assert!(unsafe { mmd_runtime_model_create_from_descriptor(&descriptor) }.is_null());
    assert!(
        last_error_cstr()
            .expect("flags error")
            .to_string_lossy()
            .contains("descriptor.flags")
    );

    descriptor.flags = 0;
    descriptor.reserved = 1;
    assert!(unsafe { mmd_runtime_model_create_from_descriptor(&descriptor) }.is_null());
    assert!(
        last_error_cstr()
            .expect("reserved error")
            .to_string_lossy()
            .contains("descriptor.reserved")
    );

    descriptor.reserved = 0;
    descriptor.bones = ptr::null();
    assert!(unsafe { mmd_runtime_model_create_from_descriptor(&descriptor) }.is_null());
    assert!(
        last_error_cstr()
            .expect("pointer/count error")
            .to_string_lossy()
            .contains("descriptor.bones")
    );

    descriptor.bones = bones.as_ptr();
    descriptor.bone_count = 0;
    assert!(unsafe { mmd_runtime_model_create_from_descriptor(&descriptor) }.is_null());
    assert!(
        last_error_cstr()
            .expect("empty bones error")
            .to_string_lossy()
            .contains("descriptor.bones")
    );

    descriptor.bone_count = bones.len();
    let model = unsafe { mmd_runtime_model_create_from_descriptor(&descriptor) };
    assert!(
        !model.is_null(),
        "a valid retry on the same thread must succeed"
    );
    assert!(
        last_error_cstr().is_none(),
        "a successful retry must clear the previous descriptor error"
    );
    unsafe { mmd_runtime_model_free(model) };
}

#[test]
fn descriptor_v1_rejects_unknown_record_flags_and_link_range() {
    let (bones, mut descriptor) = descriptor_v1_header_fixture();
    let bad_bones = [MmdRuntimeModelBoneDescriptor {
        flags: 1 << 31,
        ..bones[0]
    }];
    descriptor.bones = bad_bones.as_ptr();
    descriptor.bone_count = bad_bones.len();
    assert!(unsafe { mmd_runtime_model_create_from_descriptor(&descriptor) }.is_null());
    assert!(
        last_error_cstr()
            .expect("unknown bone flag error")
            .to_string_lossy()
            .contains("bones[0].flags")
    );

    descriptor.bones = bones.as_ptr();
    let bad_link_flags = [MmdRuntimeModelIkLinkDescriptor {
        bone_index: 0,
        flags: 1 << 31,
        angle_limit_min_xyz: [0.0; 3],
        angle_limit_max_xyz: [0.0; 3],
    }];
    descriptor.ik_links = bad_link_flags.as_ptr();
    descriptor.ik_link_count = bad_link_flags.len();
    assert!(unsafe { mmd_runtime_model_create_from_descriptor(&descriptor) }.is_null());
    assert!(
        last_error_cstr()
            .expect("unreferenced IK link flag error")
            .to_string_lossy()
            .contains("ik_links[0].flags")
    );

    descriptor.ik_links = ptr::null();
    descriptor.ik_link_count = 0;
    let bad_append_flags = [MmdRuntimeModelAppendDescriptor {
        target_bone_index: 0,
        source_bone_index: 0,
        ratio: 1.0,
        flags: 1 << 31,
    }];
    descriptor.append_transforms = bad_append_flags.as_ptr();
    descriptor.append_transform_count = bad_append_flags.len();
    assert!(unsafe { mmd_runtime_model_create_from_descriptor(&descriptor) }.is_null());
    assert!(
        last_error_cstr()
            .expect("unknown append flag error")
            .to_string_lossy()
            .contains("append_transforms[0].flags")
    );

    descriptor.append_transforms = ptr::null();
    descriptor.append_transform_count = 0;
    let links = [MmdRuntimeModelIkLinkDescriptor {
        bone_index: 0,
        flags: 0,
        angle_limit_min_xyz: [0.0; 3],
        angle_limit_max_xyz: [0.0; 3],
    }];
    let solvers = [MmdRuntimeModelIkSolverDescriptor {
        ik_bone_index: 0,
        target_bone_index: 0,
        link_offset: 1,
        link_count: 1,
        iteration_count: 1,
        limit_angle: 0.0,
    }];
    descriptor.ik_links = links.as_ptr();
    descriptor.ik_link_count = links.len();
    descriptor.ik_solvers = solvers.as_ptr();
    descriptor.ik_solver_count = solvers.len();
    assert!(unsafe { mmd_runtime_model_create_from_descriptor(&descriptor) }.is_null());
    assert!(
        last_error_cstr()
            .expect("link range error")
            .to_string_lossy()
            .contains("ik_solvers[0].links")
    );
}

#[test]
fn descriptor_v1_rejects_misaligned_and_oversized_record_ranges_before_deref() {
    let (bones, mut descriptor) = descriptor_v1_header_fixture();
    let misaligned_bones =
        unsafe { bones.as_ptr().cast::<u8>().add(1) as *const MmdRuntimeModelBoneDescriptor };
    descriptor.bones = misaligned_bones;
    descriptor.bone_count = 1;
    assert!(unsafe { mmd_runtime_model_create_from_descriptor(&descriptor) }.is_null());
    assert!(
        last_error_cstr()
            .expect("misaligned record error")
            .to_string_lossy()
            .contains("descriptor.bones: pointer is misaligned")
    );

    descriptor.bones = bones.as_ptr();
    descriptor.bone_count = (u32::MAX as usize).saturating_add(1);
    assert!(unsafe { mmd_runtime_model_create_from_descriptor(&descriptor) }.is_null());
    assert!(
        last_error_cstr()
            .expect("oversized count error")
            .to_string_lossy()
            .contains("descriptor.bones: count exceeds u32::MAX")
    );
}

#[test]
fn descriptor_feature_flag_is_available_without_changing_abi() {
    assert_eq!(mmd_runtime_abi_version(), ABI_VERSION);
    assert_ne!(
        mmd_runtime_feature_flags() & MMD_RUNTIME_FEATURE_MODEL_DESCRIPTOR,
        0
    );
    assert_ne!(
        mmd_runtime_feature_flags() & MMD_RUNTIME_FEATURE_HOST_POSE_NATIVE_MORPHS,
        0
    );
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

fn two_bone_host_pose_fixture() -> (*mut MmdRuntimeModel, *mut MmdRuntimeInstance) {
    let parents = [-1, 0];
    let rest_positions = [1.0, 0.0, 0.0, 0.0, 2.0, 0.0];
    let model = unsafe { mmd_runtime_model_create(parents.as_ptr(), rest_positions.as_ptr(), 2) };
    assert!(!model.is_null());
    let instance = unsafe { mmd_runtime_instance_create(model, 1) };
    assert!(!instance.is_null());
    (model, instance)
}

#[test]
fn apply_host_pose_rejects_null_instance_and_view() {
    let (model, instance) = two_bone_host_pose_fixture();

    let positions = [0.0f32; 6];
    let rotations = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    let scales = [1.0f32; 6];
    let morph_weights = [0.0f32];
    let view = MmdRuntimeFfiHostPoseView {
        local_position_offsets_xyz: positions.as_ptr(),
        local_rotation_xyzw: rotations.as_ptr(),
        local_scales_xyz: scales.as_ptr(),
        bone_count: 2,
        morph_weights: morph_weights.as_ptr(),
        morph_count: 1,
        ik_enabled: ptr::null(),
        ik_count: 0,
    };

    assert_eq!(
        unsafe { mmd_runtime_instance_apply_host_pose(ptr::null_mut(), &view) },
        MmdRuntimeStatus::InvalidInput
    );
    assert_eq!(
        unsafe { mmd_runtime_instance_apply_host_pose(instance, ptr::null()) },
        MmdRuntimeStatus::InvalidInput
    );
    assert_eq!(
        unsafe {
            mmd_runtime_instance_apply_host_pose_and_evaluate_before_physics(ptr::null_mut(), &view)
        },
        MmdRuntimeStatus::InvalidInput
    );
    assert_eq!(
        unsafe {
            mmd_runtime_instance_apply_host_pose_and_evaluate_before_physics(instance, ptr::null())
        },
        MmdRuntimeStatus::InvalidInput
    );

    unsafe {
        mmd_runtime_instance_free(instance);
        mmd_runtime_model_free(model);
    }
}

#[test]
fn apply_host_pose_and_evaluate_before_physics_updates_world_matrices_and_morphs() {
    let (model, instance) = two_bone_host_pose_fixture();

    let positions = [0.5, 0.0, 0.0, 0.0, 0.0, 1.0];
    let rotations = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    let scales = [1.0f32; 6];
    let morph_weights = [0.75f32];
    let view = MmdRuntimeFfiHostPoseView {
        local_position_offsets_xyz: positions.as_ptr(),
        local_rotation_xyzw: rotations.as_ptr(),
        local_scales_xyz: scales.as_ptr(),
        bone_count: 2,
        morph_weights: morph_weights.as_ptr(),
        morph_count: 1,
        ik_enabled: ptr::null(),
        ik_count: 0,
    };

    let status = unsafe {
        mmd_runtime_instance_apply_host_pose_and_evaluate_before_physics(instance, &view)
    };
    assert_eq!(status, MmdRuntimeStatus::Ok);

    let mut matrices = [0.0f32; 32];
    assert!(unsafe {
        mmd_runtime_instance_copy_world_matrices(instance, matrices.as_mut_ptr(), matrices.len())
    });
    // bone0 world position: rest (1,0,0) + offset (0.5,0,0) = (1.5,0,0)
    assert_near(matrices[12], 1.5, 1e-6);
    assert_near(matrices[13], 0.0, 1e-6);
    assert_near(matrices[14], 0.0, 1e-6);
    // bone1 world position: bone0 world (1.5,0,0) + rest (0,2,0) + offset (0,0,1.0)
    assert_near(matrices[16 + 12], 1.5, 1e-6);
    assert_near(matrices[16 + 13], 2.0, 1e-6);
    assert_near(matrices[16 + 14], 1.0, 1e-6);

    let mut copied_morphs = [0.0f32; 1];
    assert!(unsafe {
        mmd_runtime_instance_copy_morph_weights(
            instance,
            copied_morphs.as_mut_ptr(),
            copied_morphs.len(),
        )
    });
    assert_near(copied_morphs[0], 0.75, 1e-6);

    unsafe {
        mmd_runtime_instance_free(instance);
        mmd_runtime_model_free(model);
    }
}

#[test]
fn apply_host_pose_bone_count_mismatch_returns_invalid_input() {
    let (model, instance) = two_bone_host_pose_fixture();

    let positions = [0.0f32; 3];
    let rotations = [0.0, 0.0, 0.0, 1.0];
    let scales = [1.0f32; 3];
    let morph_weights = [0.0f32];
    let view = MmdRuntimeFfiHostPoseView {
        local_position_offsets_xyz: positions.as_ptr(),
        local_rotation_xyzw: rotations.as_ptr(),
        local_scales_xyz: scales.as_ptr(),
        bone_count: 1,
        morph_weights: morph_weights.as_ptr(),
        morph_count: 1,
        ik_enabled: ptr::null(),
        ik_count: 0,
    };

    let status = unsafe { mmd_runtime_instance_apply_host_pose(instance, &view) };
    assert_eq!(status, MmdRuntimeStatus::InvalidInput);
    let message = last_error_cstr().expect("expected host pose error message");
    assert!(message.to_bytes().starts_with(b"bone count mismatch"));

    // A failed apply must not mutate the pose: rest-pose evaluation should
    // still produce the model's untouched rest positions.
    assert!(unsafe { mmd_runtime_instance_evaluate_rest_pose(instance) });
    let mut matrices = [0.0f32; 32];
    assert!(unsafe {
        mmd_runtime_instance_copy_world_matrices(instance, matrices.as_mut_ptr(), matrices.len())
    });
    assert_near(matrices[12], 1.0, 1e-6);
    assert_near(matrices[16 + 12], 1.0, 1e-6);
    assert_near(matrices[16 + 13], 2.0, 1e-6);

    unsafe {
        mmd_runtime_instance_free(instance);
        mmd_runtime_model_free(model);
    }
}

#[cfg(feature = "physics-bullet-native")]
fn host_frame_pmx_bytes() -> Vec<u8> {
    let descriptor: mmd_anim_format::PmxPartsDescriptor =
        serde_json::from_value(serde_json::json!({
            "bones": [
                {"name": "root", "position": [0.0, 0.0, 0.0]},
                {"name": "anchor", "position": [0.0, 10.0, 0.0]},
                {"name": "physics", "position": [0.0, 8.0, 0.0]}
            ],
            "rigidBodies": [
                {
                    "name": "アンカー剛体",
                    "boneIndex": 1,
                    "shape": "sphere",
                    "size": [0.5, 0.0, 0.0],
                    "position": [0.0, 10.0, 0.0],
                    "friction": 0.25,
                    "mode": "static"
                },
                {
                    "name": "物理剛体",
                    "boneIndex": 2,
                    "shape": "sphere",
                    "size": [0.5, 0.0, 0.0],
                    "position": [0.0, 8.0, 0.0],
                    "mass": 1.0,
                    "linearDamping": 0.2,
                    "angularDamping": 0.3,
                    "friction": 0.4,
                    "restitution": 0.1,
                    "mode": "dynamic"
                }
            ],
            "joints": [
                {
                    "name": "接続ジョイント",
                    "type": "generic6dofSpring",
                    "rigidBodyIndexA": 0,
                    "rigidBodyIndexB": 1,
                    "position": [0.0, 9.0, 0.0],
                    "translationLowerLimit": [-1.0, -2.0, -3.0],
                    "translationUpperLimit": [1.0, 2.0, 3.0],
                    "rotationLowerLimit": [-0.1, -0.2, -0.3],
                    "rotationUpperLimit": [0.1, 0.2, 0.3],
                    "springTranslationFactor": [1.0, 2.0, 3.0],
                    "springRotationFactor": [4.0, 5.0, 6.0]
                }
            ]
        }))
        .unwrap();

    let positions_xyz = [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    let normals_xyz = [0.0f32, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0];
    let uvs_xy = [0.0f32, 0.0, 1.0, 0.0, 0.0, 1.0];
    let indices = [0u32, 1, 2];

    let model = mmd_anim_format::build_pmx_model_from_parts(mmd_anim_format::PmxPartsInput {
        descriptor,
        positions_xyz: &positions_xyz,
        normals_xyz: &normals_xyz,
        uvs_xy: &uvs_xy,
        indices: &indices,
        skin_indices: &[],
        skin_weights: &[],
        edge_scale: &[],
    })
    .unwrap();

    mmd_anim_format::export_pmx_model(&model)
}

#[cfg(feature = "physics-bullet-native")]
fn physics_params_json_bytes(world: *const MmdRuntimePhysicsWorld) -> Vec<u8> {
    let buffer = unsafe { mmd_runtime_physics_params_get_json(world) };
    assert!(!buffer.data.is_null(), "{:?}", last_error_cstr());
    assert!(buffer.len > 0);
    let bytes = unsafe { slice::from_raw_parts(buffer.data, buffer.len).to_vec() };
    unsafe { mmd_runtime_byte_buffer_free(buffer) };
    bytes
}

#[cfg(feature = "physics-bullet-native")]
fn physics_gravity(world: *const MmdRuntimePhysicsWorld) -> [f32; 3] {
    let mut gravity = [0.0; 3];
    assert_eq!(
        unsafe { mmd_runtime_physics_world_get_gravity(world, gravity.as_mut_ptr()) },
        MmdRuntimeStatus::Ok
    );
    gravity
}

#[cfg(feature = "physics-bullet-native")]
fn physics_world_from_bytes(bytes: &[u8]) -> *mut MmdRuntimePhysicsWorld {
    let mut world = ptr::null_mut();
    assert_eq!(
        unsafe {
            mmd_runtime_physics_world_create_from_pmx_bytes(bytes.as_ptr(), bytes.len(), &mut world)
        },
        MmdRuntimeStatus::Ok
    );
    assert!(!world.is_null());
    world
}

#[cfg(feature = "physics-bullet-native")]
#[test]
fn physics_params_get_emits_complete_deterministic_utf8_schema_v1_snapshot() {
    let bytes = host_frame_pmx_bytes();
    let world = physics_world_from_bytes(&bytes);

    let first = physics_params_json_bytes(world);
    let second = physics_params_json_bytes(world);
    assert_eq!(first, second);
    let value: serde_json::Value = serde_json::from_slice(&first).unwrap();
    assert_eq!(
        value.as_object().unwrap().keys().collect::<Vec<_>>(),
        ["joints", "rigid_bodies", "schema_version"]
    );
    assert_eq!(value["schema_version"], 1);
    assert_eq!(
        value["rigid_bodies"]
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>(),
        ["アンカー剛体", "物理剛体"]
    );
    assert_eq!(value["rigid_bodies"]["物理剛体"]["mass"], 1.0);
    assert_eq!(value["rigid_bodies"]["物理剛体"]["linear_damping"], 0.2);
    assert_eq!(value["rigid_bodies"]["物理剛体"]["angular_damping"], 0.3);
    assert_eq!(value["rigid_bodies"]["物理剛体"]["friction"], 0.4);
    assert_eq!(value["rigid_bodies"]["物理剛体"]["restitution"], 0.1);
    assert_eq!(
        value["joints"]["接続ジョイント"]["translation_lower_limit"],
        serde_json::json!([-1.0, -2.0, -3.0])
    );
    assert_eq!(
        value["joints"]["接続ジョイント"]["spring_rotation_factor"],
        serde_json::json!([4.0, 5.0, 6.0])
    );

    unsafe { mmd_runtime_physics_world_free(world) };
}

#[cfg(feature = "physics-bullet-native")]
#[test]
fn physics_params_unchanged_roundtrip_is_stable() {
    let bytes = host_frame_pmx_bytes();
    let world = physics_world_from_bytes(&bytes);
    let snapshot = physics_params_json_bytes(world);

    assert_eq!(
        unsafe { mmd_runtime_physics_params_set_json(world, snapshot.as_ptr(), snapshot.len()) },
        MmdRuntimeStatus::Ok
    );
    assert_eq!(physics_params_json_bytes(world), snapshot);

    unsafe { mmd_runtime_physics_world_free(world) };
}

#[cfg(feature = "physics-bullet-native")]
#[test]
fn physics_params_set_rearms_next_bake_sample_as_seed_only_via_abi() {
    let (model, instance, world) = host_frame_fixture();
    let clip = create_bone_only_physics_bake_clip();
    assert!(!clip.is_null());

    assert_eq!(
        unsafe { mmd_runtime_physics_world_reset(world, instance, ptr::null_mut()) },
        MmdRuntimeStatus::Ok
    );
    let mut report = zero_physics_step_report();
    assert_eq!(
        unsafe { mmd_runtime_physics_world_step_runtime(world, instance, 1.0 / 60.0, &mut report) },
        MmdRuntimeStatus::Ok
    );
    assert!(report.tick.substeps > 0 || report.bones_written_back > 0);

    let snapshot = physics_params_json_bytes(world);
    assert_eq!(
        unsafe { mmd_runtime_physics_params_set_json(world, snapshot.as_ptr(), snapshot.len()) },
        MmdRuntimeStatus::Ok
    );

    let mut world_out = [0.0f32; 48];
    let morph_count = unsafe { mmd_runtime_model_morph_count(model) };
    let mut morphs = vec![0.0f32; morph_count];
    report = zero_physics_step_report();
    assert_eq!(
        unsafe {
            mmd_runtime_physics_world_bake_clip_frames(
                world,
                instance,
                clip,
                0.0,
                15.0,
                1.0 / 60.0,
                1,
                world_out.as_mut_ptr(),
                world_out.len(),
                morphs.as_mut_ptr(),
                morphs.len(),
                &mut report,
            )
        },
        MmdRuntimeStatus::Ok
    );
    assert_zero_physics_step_report(&report);

    unsafe {
        mmd_runtime_clip_free(clip);
        mmd_runtime_physics_world_free(world);
        mmd_runtime_instance_free(instance);
        mmd_runtime_model_free(model);
    }
}

#[cfg(feature = "physics-bullet-native")]
#[test]
fn physics_params_partial_set_rebuilds_and_preserves_gravity() {
    let bytes = host_frame_pmx_bytes();
    let world = physics_world_from_bytes(&bytes);
    let gravity = [1.0f32, -42.0, 3.0];
    assert_eq!(
        unsafe { mmd_runtime_physics_world_set_gravity(world, gravity.as_ptr()) },
        MmdRuntimeStatus::Ok
    );
    unsafe {
        (*world).next_bake_sample_is_seed_only = false;
    }
    let update = serde_json::to_vec(&serde_json::json!({
        "schema_version": 1,
        "rigid_bodies": {
            "物理剛体": {
                "mass": 2.5,
                "linear_damping": 0.6,
                "angular_damping": 0.7,
                "friction": 0.8,
                "restitution": 0.9
            }
        },
        "joints": {
            "接続ジョイント": {
                "translation_lower_limit": [-4.0, -5.0, -6.0],
                "translation_upper_limit": [4.0, 5.0, 6.0],
                "rotation_lower_limit": [-0.4, -0.5, -0.6],
                "rotation_upper_limit": [0.4, 0.5, 0.6],
                "spring_translation_factor": [7.0, 8.0, 9.0],
                "spring_rotation_factor": [10.0, 11.0, 12.0]
            }
        }
    }))
    .unwrap();

    assert_eq!(
        unsafe { mmd_runtime_physics_params_set_json(world, update.as_ptr(), update.len()) },
        MmdRuntimeStatus::Ok
    );
    assert_eq!(physics_gravity(world), gravity);
    assert!(unsafe { (*world).next_bake_sample_is_seed_only });
    let value: serde_json::Value =
        serde_json::from_slice(&physics_params_json_bytes(world)).unwrap();
    assert_eq!(value["rigid_bodies"]["物理剛体"]["mass"], 2.5);
    assert_eq!(
        value["joints"]["接続ジョイント"]["translation_lower_limit"],
        serde_json::json!([-4.0, -5.0, -6.0])
    );
    assert_eq!(
        value["joints"]["接続ジョイント"]["spring_rotation_factor"],
        serde_json::json!([10.0, 11.0, 12.0])
    );

    unsafe { mmd_runtime_physics_world_free(world) };
}

#[cfg(feature = "physics-bullet-native")]
#[test]
fn physics_params_invalid_updates_are_fail_atomic() {
    let bytes = host_frame_pmx_bytes();
    let world = physics_world_from_bytes(&bytes);
    let gravity = [2.0f32, -50.0, 4.0];
    assert_eq!(
        unsafe { mmd_runtime_physics_world_set_gravity(world, gravity.as_ptr()) },
        MmdRuntimeStatus::Ok
    );
    let snapshot = physics_params_json_bytes(world);
    unsafe {
        (*world).next_bake_sample_is_seed_only = false;
    }
    let invalid_updates = [
        r#"{"schema_version":2}"#.as_bytes().to_vec(),
        r#"{"schema_version":1,"unknown":{}}"#.as_bytes().to_vec(),
        r#"{"schema_version":1,"rigid_bodies":{"missing":{"mass":1}}}"#
            .as_bytes()
            .to_vec(),
        r#"{"schema_version":1,"rigid_bodies":{"物理剛体":{"unknown":1}}}"#
            .as_bytes()
            .to_vec(),
        r#"{"schema_version":1,"rigid_bodies":{"物理剛体":{"mass":-1}}}"#
            .as_bytes()
            .to_vec(),
        r#"{"schema_version":1,"rigid_bodies":{"物理剛体":{"mass":null}}}"#
            .as_bytes()
            .to_vec(),
        r#"{"schema_version":1,"rigid_bodies":{"物理剛体":{"linear_damping":1.1}}}"#
            .as_bytes()
            .to_vec(),
        r#"{"schema_version":1,"rigid_bodies":{"物理剛体":{"mass":1e999}}}"#
            .as_bytes()
            .to_vec(),
        r#"{"schema_version":1,"joints":{"missing":{"spring_translation_factor":[0,0,0]}}}"#
            .as_bytes()
            .to_vec(),
        r#"{"schema_version":1,"joints":{"接続ジョイント":{"spring_rotation_factor":[0,-1,0]}}}"#
            .as_bytes()
            .to_vec(),
        r#"{"schema_version":1,"joints":{"接続ジョイント":{"spring_rotation_factor":null}}}"#
            .as_bytes()
            .to_vec(),
        r#"{"schema_version":1,"joints":{"接続ジョイント":{"translation_lower_limit":[2,0,0]}}}"#
            .as_bytes()
            .to_vec(),
        r#"{"schema_version":1"#.as_bytes().to_vec(),
        vec![0xff, 0xfe],
    ];

    for update in &invalid_updates {
        assert_eq!(
            unsafe { mmd_runtime_physics_params_set_json(world, update.as_ptr(), update.len()) },
            MmdRuntimeStatus::InvalidInput,
            "update={:?}",
            String::from_utf8_lossy(update)
        );
        assert!(last_error_cstr().is_some());
        assert_eq!(physics_params_json_bytes(world), snapshot);
        assert_eq!(physics_gravity(world), gravity);
        assert!(!unsafe { (*world).next_bake_sample_is_seed_only });
    }

    unsafe { mmd_runtime_physics_world_free(world) };
}

#[cfg(feature = "physics-bullet-native")]
#[test]
fn physics_params_reject_unnamed_and_ambiguous_worlds() {
    let valid_update = br#"{"schema_version":1}"#;

    let mut typed_world = ptr::null_mut();
    assert_eq!(
        unsafe {
            mmd_runtime_physics_world_create(ptr::null(), 0, ptr::null(), 0, &mut typed_world)
        },
        MmdRuntimeStatus::Ok
    );
    let get = unsafe { mmd_runtime_physics_params_get_json(typed_world) };
    assert!(get.data.is_null());
    assert_eq!(get.len, 0);
    assert_eq!(
        unsafe {
            mmd_runtime_physics_params_set_json(
                typed_world,
                valid_update.as_ptr(),
                valid_update.len(),
            )
        },
        MmdRuntimeStatus::Unsupported
    );
    unsafe { mmd_runtime_physics_world_free(typed_world) };

    let mutations: [fn(&mut mmd_anim_format::PmxParsedModel); 2] = [
        |model: &mut mmd_anim_format::PmxParsedModel| {
            model.rigid_bodies[1].name = model.rigid_bodies[0].name.clone();
        },
        |model: &mut mmd_anim_format::PmxParsedModel| {
            model.joints[0].name.clear();
        },
    ];
    for mutate in mutations {
        let bytes = host_frame_pmx_bytes();
        let mut model = mmd_anim_format::parse_pmx_model(&bytes).unwrap();
        mutate(&mut model);
        let bytes = mmd_anim_format::export_pmx_model(&model);
        let world = physics_world_from_bytes(&bytes);
        let get = unsafe { mmd_runtime_physics_params_get_json(world) };
        assert!(get.data.is_null());
        assert_eq!(get.len, 0);
        assert!(last_error_cstr().is_some());
        assert_eq!(
            unsafe {
                mmd_runtime_physics_params_set_json(
                    world,
                    valid_update.as_ptr(),
                    valid_update.len(),
                )
            },
            MmdRuntimeStatus::InvalidInput
        );
        unsafe { mmd_runtime_physics_world_free(world) };
    }
}

#[cfg(feature = "physics-bullet-native")]
#[test]
fn physics_param_error_preserves_embedded_nul_in_requested_pmx_name() {
    let world = physics_world_from_bytes(&host_frame_pmx_bytes());
    let update = serde_json::to_vec(&serde_json::json!({
        "schema_version": 1,
        "rigid_bodies": {"missing\0rigid-body": {"mass": 1.0}}
    }))
    .unwrap();

    let status =
        unsafe { mmd_runtime_physics_params_set_json(world, update.as_ptr(), update.len()) };
    assert_eq!(status, MmdRuntimeStatus::InvalidInput);
    let message = last_error_cstr().expect("expected unknown-name diagnostic");
    assert_eq!(
        message.to_bytes(),
        b"unknown PMX rigid body name: missing\\0rigid-body"
    );

    unsafe { mmd_runtime_physics_world_free(world) };
}

/// Builds a model + instance + physics world all derived from the same PMX
/// bytes, so rigidbody bone indices line up with the runtime skeleton. Bone 0
/// (`root`) is not bound to any rigidbody; bones 1 (`anchor`, static) and 2
/// (`physics`, dynamic) are bound and linked by a joint.
#[cfg(feature = "physics-bullet-native")]
fn host_frame_fixture() -> (
    *mut MmdRuntimeModel,
    *mut MmdRuntimeInstance,
    *mut MmdRuntimePhysicsWorld,
) {
    let bytes = host_frame_pmx_bytes();
    let model = unsafe { mmd_runtime_model_create_from_pmx_bytes(bytes.as_ptr(), bytes.len()) };
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

    let mut world = ptr::null_mut();
    assert_eq!(
        unsafe {
            mmd_runtime_physics_world_create_from_pmx_bytes(bytes.as_ptr(), bytes.len(), &mut world)
        },
        MmdRuntimeStatus::Ok
    );
    assert!(!world.is_null());

    (model, instance, world)
}

#[cfg(feature = "physics-bullet-native")]
fn host_frame_pose_view(offset_x: f32) -> ([f32; 9], [f32; 12], [f32; 9]) {
    let positions = [offset_x, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    let rotations = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    let scales = [1.0f32; 9];
    (positions, rotations, scales)
}

#[cfg(feature = "physics-bullet-native")]
#[test]
fn evaluate_host_frame_rejects_null_pointers() {
    let (model, instance) = two_bone_host_pose_fixture();
    let mut world = ptr::null_mut();
    assert_eq!(
        unsafe { mmd_runtime_physics_world_create(ptr::null(), 0, ptr::null(), 0, &mut world) },
        MmdRuntimeStatus::Ok
    );
    assert!(!world.is_null());

    let positions = [0.0f32; 6];
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
            mmd_runtime_evaluate_host_frame(
                ptr::null_mut(),
                world,
                &view,
                MmdRuntimePhysicsFrameAction::Seed as u32,
                1.0 / 60.0,
                1.0e-3,
                0,
                &mut report,
            )
        },
        MmdRuntimeStatus::InvalidInput
    );
    assert_eq!(
        unsafe {
            mmd_runtime_evaluate_host_frame(
                instance,
                ptr::null_mut(),
                &view,
                MmdRuntimePhysicsFrameAction::Seed as u32,
                1.0 / 60.0,
                1.0e-3,
                0,
                &mut report,
            )
        },
        MmdRuntimeStatus::InvalidInput
    );
    assert_eq!(
        unsafe {
            mmd_runtime_evaluate_host_frame(
                instance,
                world,
                ptr::null(),
                MmdRuntimePhysicsFrameAction::Seed as u32,
                1.0 / 60.0,
                1.0e-3,
                0,
                &mut report,
            )
        },
        MmdRuntimeStatus::InvalidInput
    );

    unsafe {
        mmd_runtime_physics_world_free(world);
        mmd_runtime_instance_free(instance);
        mmd_runtime_model_free(model);
    }
}

#[cfg(feature = "physics-bullet-native")]
#[test]
fn evaluate_host_frame_seed_evaluates_pose() {
    let (model, instance, world) = host_frame_fixture();

    let (positions, rotations, scales) = host_frame_pose_view(0.3);
    let view = MmdRuntimeFfiHostPoseView {
        local_position_offsets_xyz: positions.as_ptr(),
        local_rotation_xyzw: rotations.as_ptr(),
        local_scales_xyz: scales.as_ptr(),
        bone_count: 3,
        morph_weights: ptr::null(),
        morph_count: 0,
        ik_enabled: ptr::null(),
        ik_count: 0,
    };

    let mut report = MmdRuntimeFfiPhysicsWorldStepReport {
        tick: MmdRuntimeFfiPhysicsStepStats {
            input_dt_seconds: -1.0,
            clamped_dt_seconds: -1.0,
            substeps: u32::MAX,
            accumulator_seconds: -1.0,
        },
        kinematic_rigidbodies_fed: usize::MAX,
        bones_written_back: usize::MAX,
    };

    let status = unsafe {
        mmd_runtime_evaluate_host_frame(
            instance,
            world,
            &view,
            MmdRuntimePhysicsFrameAction::Seed as u32,
            1.0 / 60.0,
            1.0e-3,
            0,
            &mut report,
        )
    };
    assert_eq!(status, MmdRuntimeStatus::Ok);

    // A seed does not advance the solver, so the report carries no
    // meaningful statistics.
    assert_eq!(report.tick.substeps, 0);
    assert_eq!(report.kinematic_rigidbodies_fed, 0);
    assert_eq!(report.bones_written_back, 0);

    // Bone 0 (`root`) is not bound to any rigidbody, so its world position
    // must reflect the injected host pose offset through before-physics
    // evaluation, surviving the seed's readback pass untouched.
    let mut matrices = [0.0f32; 48];
    assert!(unsafe {
        mmd_runtime_instance_copy_world_matrices(instance, matrices.as_mut_ptr(), matrices.len())
    });
    assert_near(matrices[12], 0.3, 1e-4);
    assert_near(matrices[13], 0.0, 1e-4);
    assert_near(matrices[14], 0.0, 1e-4);

    unsafe {
        mmd_runtime_physics_world_free(world);
        mmd_runtime_instance_free(instance);
        mmd_runtime_model_free(model);
    }
}

#[cfg(feature = "physics-bullet-native")]
#[test]
fn evaluate_host_frame_step_produces_report() {
    let (model, instance, world) = host_frame_fixture();

    let (positions, rotations, scales) = host_frame_pose_view(0.0);
    let view = MmdRuntimeFfiHostPoseView {
        local_position_offsets_xyz: positions.as_ptr(),
        local_rotation_xyzw: rotations.as_ptr(),
        local_scales_xyz: scales.as_ptr(),
        bone_count: 3,
        morph_weights: ptr::null(),
        morph_count: 0,
        ik_enabled: ptr::null(),
        ik_count: 0,
    };
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
            mmd_runtime_evaluate_host_frame(
                instance,
                world,
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

    let status = unsafe {
        mmd_runtime_evaluate_host_frame(
            instance,
            world,
            &view,
            MmdRuntimePhysicsFrameAction::Step as u32,
            1.0 / 60.0,
            1.0e-3,
            0,
            &mut report,
        )
    };
    assert_eq!(status, MmdRuntimeStatus::Ok);
    assert!(
        report.kinematic_rigidbodies_fed > 0 || report.bones_written_back > 0,
        "expected step to feed or write back at least one rigidbody: {report:?}"
    );

    unsafe {
        mmd_runtime_physics_world_free(world);
        mmd_runtime_instance_free(instance);
        mmd_runtime_model_free(model);
    }
}

/// Two independent (model, instance, world) triples must never share mutable
/// state: driving one with `mmd_runtime_evaluate_host_frame` must not leak
/// into the other's evaluated pose, even when both are seeded and stepped
/// interleaved on the same thread.
#[cfg(feature = "physics-bullet-native")]
#[test]
fn two_independent_worlds_do_not_cross_contaminate() {
    let (model_a, instance_a, world_a) = host_frame_fixture();
    let (model_b, instance_b, world_b) = host_frame_fixture();

    let (positions_a, rotations_a, scales_a) = host_frame_pose_view(0.3);
    let view_a = MmdRuntimeFfiHostPoseView {
        local_position_offsets_xyz: positions_a.as_ptr(),
        local_rotation_xyzw: rotations_a.as_ptr(),
        local_scales_xyz: scales_a.as_ptr(),
        bone_count: 3,
        morph_weights: ptr::null(),
        morph_count: 0,
        ik_enabled: ptr::null(),
        ik_count: 0,
    };
    let (positions_b, rotations_b, scales_b) = host_frame_pose_view(-0.7);
    let view_b = MmdRuntimeFfiHostPoseView {
        local_position_offsets_xyz: positions_b.as_ptr(),
        local_rotation_xyzw: rotations_b.as_ptr(),
        local_scales_xyz: scales_b.as_ptr(),
        bone_count: 3,
        morph_weights: ptr::null(),
        morph_count: 0,
        ik_enabled: ptr::null(),
        ik_count: 0,
    };

    let mut report = zero_physics_step_report();

    assert_eq!(
        unsafe {
            mmd_runtime_evaluate_host_frame(
                instance_a,
                world_a,
                &view_a,
                MmdRuntimePhysicsFrameAction::Seed as u32,
                1.0 / 60.0,
                1.0e-3,
                0,
                &mut report,
            )
        },
        MmdRuntimeStatus::Ok
    );
    assert_eq!(
        unsafe {
            mmd_runtime_evaluate_host_frame(
                instance_b,
                world_b,
                &view_b,
                MmdRuntimePhysicsFrameAction::Seed as u32,
                1.0 / 60.0,
                1.0e-3,
                0,
                &mut report,
            )
        },
        MmdRuntimeStatus::Ok
    );
    assert_eq!(
        unsafe {
            mmd_runtime_evaluate_host_frame(
                instance_a,
                world_a,
                &view_a,
                MmdRuntimePhysicsFrameAction::Step as u32,
                1.0 / 60.0,
                1.0e-3,
                0,
                &mut report,
            )
        },
        MmdRuntimeStatus::Ok
    );
    assert_eq!(
        unsafe {
            mmd_runtime_evaluate_host_frame(
                instance_b,
                world_b,
                &view_b,
                MmdRuntimePhysicsFrameAction::Step as u32,
                1.0 / 60.0,
                1.0e-3,
                0,
                &mut report,
            )
        },
        MmdRuntimeStatus::Ok
    );

    // Bone 0 (`root`) is unbound to any rigidbody, so its world position
    // directly reflects the host pose offset injected for that world only.
    let mut matrices_a = [0.0f32; 48];
    assert!(unsafe {
        mmd_runtime_instance_copy_world_matrices(
            instance_a,
            matrices_a.as_mut_ptr(),
            matrices_a.len(),
        )
    });
    assert_near(matrices_a[12], 0.3, 1e-4);
    assert_near(matrices_a[13], 0.0, 1e-4);
    assert_near(matrices_a[14], 0.0, 1e-4);

    let mut matrices_b = [0.0f32; 48];
    assert!(unsafe {
        mmd_runtime_instance_copy_world_matrices(
            instance_b,
            matrices_b.as_mut_ptr(),
            matrices_b.len(),
        )
    });
    assert_near(matrices_b[12], -0.7, 1e-4);
    assert_near(matrices_b[13], 0.0, 1e-4);
    assert_near(matrices_b[14], 0.0, 1e-4);

    unsafe {
        mmd_runtime_physics_world_free(world_a);
        mmd_runtime_instance_free(instance_a);
        mmd_runtime_model_free(model_a);
        mmd_runtime_physics_world_free(world_b);
        mmd_runtime_instance_free(instance_b);
        mmd_runtime_model_free(model_b);
    }
}

/// A call that fails validation (here, a NULL pose) must leave the instance
/// and world handles fully usable: the very next call with valid arguments
/// must succeed and produce the expected pose, proving error recovery holds.
#[cfg(feature = "physics-bullet-native")]
#[test]
fn invalid_call_does_not_prevent_subsequent_valid_call() {
    let (model, instance, world) = host_frame_fixture();

    let mut report = zero_physics_step_report();

    let invalid_status = unsafe {
        mmd_runtime_evaluate_host_frame(
            instance,
            world,
            ptr::null(),
            MmdRuntimePhysicsFrameAction::Seed as u32,
            1.0 / 60.0,
            1.0e-3,
            0,
            &mut report,
        )
    };
    assert_eq!(invalid_status, MmdRuntimeStatus::InvalidInput);

    let (positions, rotations, scales) = host_frame_pose_view(0.42);
    let view = MmdRuntimeFfiHostPoseView {
        local_position_offsets_xyz: positions.as_ptr(),
        local_rotation_xyzw: rotations.as_ptr(),
        local_scales_xyz: scales.as_ptr(),
        bone_count: 3,
        morph_weights: ptr::null(),
        morph_count: 0,
        ik_enabled: ptr::null(),
        ik_count: 0,
    };

    let valid_status = unsafe {
        mmd_runtime_evaluate_host_frame(
            instance,
            world,
            &view,
            MmdRuntimePhysicsFrameAction::Seed as u32,
            1.0 / 60.0,
            1.0e-3,
            0,
            &mut report,
        )
    };
    assert_eq!(valid_status, MmdRuntimeStatus::Ok);

    let mut matrices = [0.0f32; 48];
    assert!(unsafe {
        mmd_runtime_instance_copy_world_matrices(instance, matrices.as_mut_ptr(), matrices.len())
    });
    assert_near(matrices[12], 0.42, 1e-4);
    assert_near(matrices[13], 0.0, 1e-4);
    assert_near(matrices[14], 0.0, 1e-4);

    unsafe {
        mmd_runtime_physics_world_free(world);
        mmd_runtime_instance_free(instance);
        mmd_runtime_model_free(model);
    }
}

/// SEED must reseed rigid bodies from the evaluated pose without advancing
/// the Bullet solver. Seeding the same host pose twice in a row must
/// therefore produce byte-identical dynamic-bone world matrices, since there
/// is no solver integration between the two calls to introduce drift.
#[cfg(feature = "physics-bullet-native")]
#[test]
fn evaluate_host_frame_seed_does_not_advance_solver() {
    let (model, instance, world) = host_frame_fixture();

    let (positions, rotations, scales) = host_frame_pose_view(0.0);
    let view = MmdRuntimeFfiHostPoseView {
        local_position_offsets_xyz: positions.as_ptr(),
        local_rotation_xyzw: rotations.as_ptr(),
        local_scales_xyz: scales.as_ptr(),
        bone_count: 3,
        morph_weights: ptr::null(),
        morph_count: 0,
        ik_enabled: ptr::null(),
        ik_count: 0,
    };

    let mut report = zero_physics_step_report();
    assert_eq!(
        unsafe {
            mmd_runtime_evaluate_host_frame(
                instance,
                world,
                &view,
                MmdRuntimePhysicsFrameAction::Seed as u32,
                1.0 / 60.0,
                1.0e-3,
                0,
                &mut report,
            )
        },
        MmdRuntimeStatus::Ok
    );
    assert_eq!(report.tick.substeps, 0);

    let mut matrices_first = [0.0f32; 48];
    assert!(unsafe {
        mmd_runtime_instance_copy_world_matrices(
            instance,
            matrices_first.as_mut_ptr(),
            matrices_first.len(),
        )
    });

    let status = unsafe {
        mmd_runtime_evaluate_host_frame(
            instance,
            world,
            &view,
            MmdRuntimePhysicsFrameAction::Seed as u32,
            1.0 / 60.0,
            1.0e-3,
            0,
            &mut report,
        )
    };
    assert_eq!(status, MmdRuntimeStatus::Ok);
    assert_eq!(report.tick.substeps, 0);

    let mut matrices_second = [0.0f32; 48];
    assert!(unsafe {
        mmd_runtime_instance_copy_world_matrices(
            instance,
            matrices_second.as_mut_ptr(),
            matrices_second.len(),
        )
    });

    // Bone 2 (`physics`, dynamic) world translation must match exactly across
    // both seeds: no solver step ran between them.
    assert_eq!(
        matrices_first[2 * 16 + 12..2 * 16 + 15],
        matrices_second[2 * 16 + 12..2 * 16 + 15]
    );

    unsafe {
        mmd_runtime_physics_world_free(world);
        mmd_runtime_instance_free(instance);
        mmd_runtime_model_free(model);
    }
}

/// STEP must be rejected while the instance's physics mode is `Off` (the
/// default), and must succeed once the mode is switched to `Trace`.
#[cfg(feature = "physics-bullet-native")]
#[test]
fn evaluate_host_frame_step_rejects_physics_mode_off() {
    let (model, instance) = two_bone_host_pose_fixture();
    let mut world = ptr::null_mut();
    assert_eq!(
        unsafe { mmd_runtime_physics_world_create(ptr::null(), 0, ptr::null(), 0, &mut world) },
        MmdRuntimeStatus::Ok
    );
    assert!(!world.is_null());

    let positions = [0.0f32; 6];
    let rotations = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    let scales = [1.0f32; 6];
    let morph_weights = [0.0f32];
    let view = MmdRuntimeFfiHostPoseView {
        local_position_offsets_xyz: positions.as_ptr(),
        local_rotation_xyzw: rotations.as_ptr(),
        local_scales_xyz: scales.as_ptr(),
        bone_count: 2,
        morph_weights: morph_weights.as_ptr(),
        morph_count: 1,
        ik_enabled: ptr::null(),
        ik_count: 0,
    };
    let mut report = zero_physics_step_report();

    // Capture world matrices before the rejected call.
    let mat_len = unsafe { mmd_runtime_instance_world_matrix_f32_len(instance) };
    let mut matrices_before = vec![0.0f32; mat_len];
    unsafe {
        mmd_runtime_instance_copy_world_matrices(instance, matrices_before.as_mut_ptr(), mat_len);
    }

    let off_status = unsafe {
        mmd_runtime_evaluate_host_frame(
            instance,
            world,
            &view,
            MmdRuntimePhysicsFrameAction::Step as u32,
            1.0 / 60.0,
            1.0e-3,
            0,
            &mut report,
        )
    };
    assert_eq!(off_status, MmdRuntimeStatus::InvalidInput);

    // Pose must not have been mutated by the rejected call.
    let mut matrices_after = vec![0.0f32; mat_len];
    unsafe {
        mmd_runtime_instance_copy_world_matrices(instance, matrices_after.as_mut_ptr(), mat_len);
    }
    assert_eq!(
        matrices_before, matrices_after,
        "pose must not change on mode-Off rejection"
    );

    assert_eq!(
        unsafe {
            mmd_runtime_instance_set_physics_mode(instance, MmdRuntimeFfiPhysicsMode::Trace as u32)
        },
        MmdRuntimeStatus::Ok
    );

    let trace_status = unsafe {
        mmd_runtime_evaluate_host_frame(
            instance,
            world,
            &view,
            MmdRuntimePhysicsFrameAction::Step as u32,
            1.0 / 60.0,
            1.0e-3,
            0,
            &mut report,
        )
    };
    assert_eq!(trace_status, MmdRuntimeStatus::Ok);

    unsafe {
        mmd_runtime_physics_world_free(world);
        mmd_runtime_instance_free(instance);
        mmd_runtime_model_free(model);
    }
}

/// `mmd_runtime_evaluate_host_frame` must reject a physics world whose
/// rigidbody bindings reference bone indices outside the instance's bone
/// range, rather than silently reading or writing out of bounds.
#[cfg(feature = "physics-bullet-native")]
#[test]
fn evaluate_host_frame_rejects_incompatible_world() {
    let (model, instance) = two_bone_host_pose_fixture();

    // `host_frame_pmx_bytes` binds rigidbodies to bone indices 1 and 2,
    // requiring at least 3 bones; the instance above only has 2.
    let bytes = host_frame_pmx_bytes();
    let mut world = ptr::null_mut();
    assert_eq!(
        unsafe {
            mmd_runtime_physics_world_create_from_pmx_bytes(bytes.as_ptr(), bytes.len(), &mut world)
        },
        MmdRuntimeStatus::Ok
    );
    assert!(!world.is_null());

    let positions = [0.0f32; 6];
    let rotations = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
    let scales = [1.0f32; 6];
    let morph_weights = [0.0f32];
    let view = MmdRuntimeFfiHostPoseView {
        local_position_offsets_xyz: positions.as_ptr(),
        local_rotation_xyzw: rotations.as_ptr(),
        local_scales_xyz: scales.as_ptr(),
        bone_count: 2,
        morph_weights: morph_weights.as_ptr(),
        morph_count: 1,
        ik_enabled: ptr::null(),
        ik_count: 0,
    };
    let mut report = zero_physics_step_report();

    let mat_len = unsafe { mmd_runtime_instance_world_matrix_f32_len(instance) };
    let mut matrices_before = vec![0.0f32; mat_len];
    unsafe {
        mmd_runtime_instance_copy_world_matrices(instance, matrices_before.as_mut_ptr(), mat_len);
    }

    let status = unsafe {
        mmd_runtime_evaluate_host_frame(
            instance,
            world,
            &view,
            MmdRuntimePhysicsFrameAction::Seed as u32,
            1.0 / 60.0,
            1.0e-3,
            0,
            &mut report,
        )
    };
    assert_eq!(status, MmdRuntimeStatus::InvalidInput);
    let message = last_error_cstr().expect("expected bone-count mismatch error message");
    assert_eq!(
        message.to_bytes(),
        b"physics_world.rigidbodies[1].bone_index: 2 exceeds instance bone_count 2"
    );

    let mut matrices_after = vec![0.0f32; mat_len];
    unsafe {
        mmd_runtime_instance_copy_world_matrices(instance, matrices_after.as_mut_ptr(), mat_len);
    }
    assert_eq!(
        matrices_before, matrices_after,
        "pose must not change on bone-count incompatibility rejection"
    );

    unsafe {
        mmd_runtime_physics_world_free(world);
        mmd_runtime_instance_free(instance);
        mmd_runtime_model_free(model);
    }
}

/// The bone-mask query must reject a caller buffer shorter than the physics
/// world's required bone count rather than silently ignoring out-of-range
/// bindings.
#[cfg(feature = "physics-bullet-native")]
#[test]
fn physics_driven_bone_mask_rejects_short_buffer() {
    // `host_frame_pmx_bytes` binds rigidbodies to bone indices 1 and 2,
    // so the world requires at least 3 bones.
    let bytes = host_frame_pmx_bytes();
    let mut world = ptr::null_mut();
    assert_eq!(
        unsafe {
            mmd_runtime_physics_world_create_from_pmx_bytes(bytes.as_ptr(), bytes.len(), &mut world)
        },
        MmdRuntimeStatus::Ok
    );
    assert!(!world.is_null());

    let mut mask = [0xffu8; 1];
    let status = unsafe {
        mmd_runtime_physics_world_physics_driven_bone_mask(world, mask.as_mut_ptr(), mask.len())
    };
    assert_eq!(status, MmdRuntimeStatus::BufferTooSmall);

    unsafe {
        mmd_runtime_physics_world_free(world);
    }
}

struct DescriptorParityModel(*mut MmdRuntimeModel);

impl Drop for DescriptorParityModel {
    fn drop(&mut self) {
        unsafe { mmd_runtime_model_free(self.0) };
    }
}

struct DescriptorParityInstance(*mut MmdRuntimeInstance);

impl Drop for DescriptorParityInstance {
    fn drop(&mut self) {
        unsafe { mmd_runtime_instance_free(self.0) };
    }
}

struct DescriptorParityClip(*mut MmdRuntimeClip);

impl Drop for DescriptorParityClip {
    fn drop(&mut self) {
        unsafe { mmd_runtime_clip_free(self.0) };
    }
}

#[cfg(feature = "physics-bullet-native")]
struct DescriptorParityWorld(*mut MmdRuntimePhysicsWorld);

#[cfg(feature = "physics-bullet-native")]
impl Drop for DescriptorParityWorld {
    fn drop(&mut self) {
        unsafe { mmd_runtime_physics_world_free(self.0) };
    }
}

struct OwnedDescriptorParityFixture {
    parsed: mmd_anim_format::pmx::PmxParsedModel,
    bones: Vec<MmdRuntimeModelBoneDescriptor>,
    ik_solvers: Vec<MmdRuntimeModelIkSolverDescriptor>,
    ik_links: Vec<MmdRuntimeModelIkLinkDescriptor>,
    append_transforms: Vec<MmdRuntimeModelAppendDescriptor>,
    bone_morph_offsets: Vec<MmdRuntimeModelBoneMorphOffsetDescriptor>,
    group_morph_offsets: Vec<MmdRuntimeModelGroupMorphOffsetDescriptor>,
}

impl OwnedDescriptorParityFixture {
    fn from_tracked_pmx() -> Self {
        let bytes: &[u8] =
            include_bytes!("../../mmd-anim-format/fixtures/pmx/model_descriptor_parity.pmx");
        let parsed = mmd_anim_format::parse_pmx_model(bytes).expect("descriptor parity PMX");
        let bones = parsed
            .skeleton
            .bones
            .iter()
            .map(|bone| {
                let mut flags = 0;
                if bone.flags.transform_after_physics {
                    flags |= MODEL_BONE_FLAG_TRANSFORM_AFTER_PHYSICS;
                }
                if bone.flags.fixed_axis {
                    flags |= MODEL_BONE_FLAG_FIXED_AXIS;
                }
                if bone.flags.local_axis {
                    flags |= MODEL_BONE_FLAG_LOCAL_AXIS;
                }
                MmdRuntimeModelBoneDescriptor {
                    parent_index: bone.parent_index,
                    rest_position_xyz: bone.position,
                    transform_order: bone.layer,
                    flags,
                    fixed_axis_xyz: bone.fixed_axis.unwrap_or([0.0; 3]),
                    local_axis_x_xyz: bone.local_axis.as_ref().map_or([0.0; 3], |axis| axis.x),
                    local_axis_z_xyz: bone.local_axis.as_ref().map_or([0.0; 3], |axis| axis.z),
                }
            })
            .collect::<Vec<_>>();

        let mut ik_solvers = Vec::new();
        let mut ik_links = Vec::new();
        let mut append_transforms = Vec::new();
        for (bone_index, bone) in parsed.skeleton.bones.iter().enumerate() {
            if let Some(ik) = &bone.ik {
                let link_offset = ik_links.len();
                for link in &ik.links {
                    let (flags, min, max) = link
                        .limits
                        .as_ref()
                        .map_or((0, [0.0; 3], [0.0; 3]), |limit| {
                            (IK_LINK_FLAG_ANGLE_LIMIT, limit.lower, limit.upper)
                        });
                    ik_links.push(MmdRuntimeModelIkLinkDescriptor {
                        bone_index: u32::try_from(link.bone_index).expect("valid IK link bone"),
                        flags,
                        angle_limit_min_xyz: min,
                        angle_limit_max_xyz: max,
                    });
                }
                ik_solvers.push(MmdRuntimeModelIkSolverDescriptor {
                    ik_bone_index: bone_index as u32,
                    target_bone_index: u32::try_from(ik.target_index)
                        .expect("valid IK target bone"),
                    link_offset,
                    link_count: ik.links.len(),
                    iteration_count: ik.loop_count.max(0) as u32,
                    limit_angle: ik.limit_angle,
                });
            }
            if let Some(append) = &bone.append_transform {
                let mut flags = 0;
                if bone.flags.append_rotate {
                    flags |= APPEND_FLAG_ROTATION;
                }
                if bone.flags.append_translate {
                    flags |= APPEND_FLAG_TRANSLATION;
                }
                if bone.flags.append_local {
                    flags |= APPEND_FLAG_LOCAL;
                }
                append_transforms.push(MmdRuntimeModelAppendDescriptor {
                    target_bone_index: bone_index as u32,
                    source_bone_index: u32::try_from(append.parent_index)
                        .expect("valid append source bone"),
                    ratio: append.weight,
                    flags,
                });
            }
        }

        let mut bone_morph_offsets = Vec::new();
        let mut group_morph_offsets = Vec::new();
        for (morph_index, morph) in parsed.morphs.iter().enumerate() {
            for offset in &morph.bone_offsets {
                bone_morph_offsets.push(MmdRuntimeModelBoneMorphOffsetDescriptor {
                    morph_index: morph_index as u32,
                    target_bone_index: u32::try_from(offset.bone_index)
                        .expect("valid bone morph target"),
                    position_offset_xyz: offset.translation,
                    rotation_offset_xyzw: offset.rotation,
                });
            }
            for offset in &morph.group_offsets {
                group_morph_offsets.push(MmdRuntimeModelGroupMorphOffsetDescriptor {
                    morph_index: morph_index as u32,
                    child_morph_index: u32::try_from(offset.morph_index)
                        .expect("valid group morph child"),
                    ratio: offset.weight,
                });
            }
        }
        Self {
            parsed,
            bones,
            ik_solvers,
            ik_links,
            append_transforms,
            bone_morph_offsets,
            group_morph_offsets,
        }
    }

    fn descriptor(&self) -> MmdRuntimeModelDescriptor {
        MmdRuntimeModelDescriptor {
            struct_size: std::mem::size_of::<MmdRuntimeModelDescriptor>() as u32,
            descriptor_version: MMD_RUNTIME_MODEL_DESCRIPTOR_VERSION_V1,
            flags: MMD_RUNTIME_MODEL_DESCRIPTOR_FLAGS_NONE,
            reserved: 0,
            bones: self.bones.as_ptr(),
            bone_count: self.bones.len(),
            ik_solvers: self.ik_solvers.as_ptr(),
            ik_solver_count: self.ik_solvers.len(),
            ik_links: self.ik_links.as_ptr(),
            ik_link_count: self.ik_links.len(),
            append_transforms: self.append_transforms.as_ptr(),
            append_transform_count: self.append_transforms.len(),
            morph_count: self.parsed.morphs.len() as u32,
            bone_morph_offsets: self.bone_morph_offsets.as_ptr(),
            bone_morph_offset_count: self.bone_morph_offsets.len(),
            group_morph_offsets: self.group_morph_offsets.as_ptr(),
            group_morph_offset_count: self.group_morph_offsets.len(),
        }
    }

    fn assert_shape(&self) {
        assert_eq!(self.parsed.metadata.counts.bones, 3);
        assert_eq!(self.parsed.metadata.counts.morphs, 2);
        assert_eq!(
            self.bones.len(),
            3,
            "fixture must exercise all bone metadata"
        );
        assert_eq!(self.ik_solvers.len(), 1, "fixture must exercise IK");
        assert!(
            !self.ik_links.is_empty(),
            "fixture must exercise an IK link"
        );
        assert_eq!(
            self.append_transforms.len(),
            1,
            "fixture must exercise append"
        );
        assert_eq!(
            self.bone_morph_offsets.len(),
            1,
            "fixture must exercise bone morph"
        );
        assert_eq!(
            self.group_morph_offsets.len(),
            1,
            "fixture must exercise group morph"
        );
        assert!(
            self.bones
                .iter()
                .any(|bone| { bone.flags & MODEL_BONE_FLAG_FIXED_AXIS != 0 })
        );
        assert!(
            self.bones
                .iter()
                .any(|bone| { bone.flags & MODEL_BONE_FLAG_LOCAL_AXIS != 0 })
        );
        assert!(
            self.bones
                .iter()
                .any(|bone| { bone.flags & MODEL_BONE_FLAG_TRANSFORM_AFTER_PHYSICS != 0 })
        );
    }
}

#[test]
fn model_descriptor_fixture_matches_pmx_model_arena_and_behavior() {
    let bytes: &[u8] =
        include_bytes!("../../mmd-anim-format/fixtures/pmx/model_descriptor_parity.pmx");
    let fixture = OwnedDescriptorParityFixture::from_tracked_pmx();
    fixture.assert_shape();
    let parsed = &fixture.parsed;
    let bones = &fixture.bones;
    let bone_morph_offsets = &fixture.bone_morph_offsets;
    let group_morph_offsets = &fixture.group_morph_offsets;
    let descriptor = fixture.descriptor();

    let pmx_model = DescriptorParityModel(unsafe {
        mmd_runtime_model_create_from_pmx_bytes(bytes.as_ptr(), bytes.len())
    });
    assert!(!pmx_model.0.is_null(), "PMX model fixture import failed");
    let descriptor_model =
        DescriptorParityModel(unsafe { mmd_runtime_model_create_from_descriptor(&descriptor) });
    assert!(
        !descriptor_model.0.is_null(),
        "descriptor fixture compile failed"
    );

    let pmx_arena = unsafe { &(*pmx_model.0).model };
    let descriptor_arena = unsafe { &(*descriptor_model.0).model };
    assert_eq!(pmx_arena.bone_count(), descriptor_arena.bone_count());
    assert_eq!(pmx_arena.eval_order(), descriptor_arena.eval_order());
    assert_eq!(pmx_arena.ik_solvers(), descriptor_arena.ik_solvers());
    assert_eq!(
        pmx_arena.append_transforms(),
        descriptor_arena.append_transforms()
    );
    assert_eq!(pmx_arena.morph_count(), descriptor_arena.morph_count());
    assert_eq!(
        pmx_arena.bone_morph_spans(),
        descriptor_arena.bone_morph_spans()
    );
    assert_eq!(
        pmx_arena.bone_morph_offsets(),
        descriptor_arena.bone_morph_offsets()
    );
    assert_eq!(
        pmx_arena.group_morph_spans(),
        descriptor_arena.group_morph_spans()
    );
    assert_eq!(
        pmx_arena.group_morph_offsets(),
        descriptor_arena.group_morph_offsets()
    );
    for bone_index in 0..pmx_arena.bone_count() {
        let bone = BoneIndex(bone_index as u32);
        assert_eq!(
            pmx_arena.parent_index(bone),
            descriptor_arena.parent_index(bone)
        );
        assert_eq!(
            pmx_arena.rest_position(bone),
            descriptor_arena.rest_position(bone)
        );
        assert_eq!(
            pmx_arena.inverse_bind_matrix(bone),
            descriptor_arena.inverse_bind_matrix(bone)
        );
        assert_eq!(
            pmx_arena.transform_order(bone),
            descriptor_arena.transform_order(bone)
        );
        assert_eq!(
            pmx_arena.fixed_axis(bone),
            descriptor_arena.fixed_axis(bone)
        );
        assert_eq!(
            pmx_arena.local_axis(bone),
            descriptor_arena.local_axis(bone)
        );
        assert_eq!(
            pmx_arena.local_axis_basis(bone),
            descriptor_arena.local_axis_basis(bone)
        );
        assert_eq!(
            pmx_arena.transform_after_physics(bone),
            descriptor_arena.transform_after_physics(bone)
        );
        assert_eq!(
            pmx_arena.append_transform_index(bone),
            descriptor_arena.append_transform_index(bone)
        );
    }

    let pmx_instance =
        DescriptorParityInstance(unsafe { mmd_runtime_instance_create(pmx_model.0, 0) });
    let descriptor_instance =
        DescriptorParityInstance(unsafe { mmd_runtime_instance_create(descriptor_model.0, 0) });
    assert!(!pmx_instance.0.is_null());
    assert!(!descriptor_instance.0.is_null());
    assert!(unsafe { mmd_runtime_instance_evaluate_rest_pose(pmx_instance.0) });
    assert!(unsafe { mmd_runtime_instance_evaluate_rest_pose(descriptor_instance.0) });
    let matrix_len = bones.len() * 16;
    let mut pmx_world = vec![0.0; matrix_len];
    let mut descriptor_world = vec![0.0; matrix_len];
    let mut pmx_skinning = vec![0.0; matrix_len];
    let mut descriptor_skinning = vec![0.0; matrix_len];
    assert!(unsafe {
        mmd_runtime_instance_copy_world_matrices(
            pmx_instance.0,
            pmx_world.as_mut_ptr(),
            pmx_world.len(),
        )
    });
    assert!(unsafe {
        mmd_runtime_instance_copy_world_matrices(
            descriptor_instance.0,
            descriptor_world.as_mut_ptr(),
            descriptor_world.len(),
        )
    });
    assert!(unsafe {
        mmd_runtime_instance_copy_skinning_matrices(
            pmx_instance.0,
            pmx_skinning.as_mut_ptr(),
            pmx_skinning.len(),
        )
    });
    assert!(unsafe {
        mmd_runtime_instance_copy_skinning_matrices(
            descriptor_instance.0,
            descriptor_skinning.as_mut_ptr(),
            descriptor_skinning.len(),
        )
    });
    assert_eq!(pmx_world, descriptor_world, "rest world matrix parity");
    assert_eq!(
        pmx_skinning, descriptor_skinning,
        "rest skinning matrix parity"
    );

    let root_rotation = glam::Quat::from_rotation_z(0.31).to_array();
    let controller_rotation = glam::Quat::from_rotation_x(-0.22).to_array();
    let bone_tracks = [
        MmdRuntimeFfiBoneTrack {
            bone_index: 0,
            keyframe_offset: 0,
            keyframe_count: 1,
        },
        MmdRuntimeFfiBoneTrack {
            bone_index: 1,
            keyframe_offset: 1,
            keyframe_count: 1,
        },
        MmdRuntimeFfiBoneTrack {
            bone_index: 2,
            keyframe_offset: 2,
            keyframe_count: 1,
        },
    ];
    let bone_keyframes = [
        MmdRuntimeFfiBoneKeyframe {
            frame: 0,
            position_xyz: [0.13, -0.04, 0.02],
            rotation_xyzw: root_rotation,
        },
        MmdRuntimeFfiBoneKeyframe {
            frame: 0,
            position_xyz: [0.0, 0.11, 0.0],
            rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
        },
        MmdRuntimeFfiBoneKeyframe {
            frame: 0,
            position_xyz: [0.06, 0.0, 0.0],
            rotation_xyzw: controller_rotation,
        },
    ];
    let morph_tracks = [
        MmdRuntimeFfiMorphTrack {
            morph_index: 0,
            keyframe_offset: 0,
            keyframe_count: 1,
        },
        MmdRuntimeFfiMorphTrack {
            morph_index: 1,
            keyframe_offset: 1,
            keyframe_count: 1,
        },
    ];
    let morph_keyframes = [
        MmdRuntimeFfiMorphKeyframe {
            frame: 0,
            weight: 0.25,
        },
        MmdRuntimeFfiMorphKeyframe {
            frame: 0,
            weight: 0.75,
        },
    ];
    let property_keyframes = [MmdRuntimeFfiPropertyKeyframe {
        frame: 0,
        ik_enabled_offset: 0,
        ik_enabled_count: 1,
    }];
    let make_clip = |bone_keyframes: &[MmdRuntimeFfiBoneKeyframe],
                     morph_keyframes: &[MmdRuntimeFfiMorphKeyframe],
                     property_keyframes: &[MmdRuntimeFfiPropertyKeyframe],
                     property_ik_enabled: &[u8]| {
        DescriptorParityClip(unsafe {
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
        })
    };
    let property_ik_enabled = [1u8];
    let clip = make_clip(
        &bone_keyframes,
        &morph_keyframes,
        &property_keyframes,
        &property_ik_enabled,
    );
    assert!(!clip.0.is_null(), "typed parity clip creation failed");
    assert!(unsafe { mmd_runtime_instance_evaluate_clip_frame(pmx_instance.0, clip.0, 0.0) });
    assert!(unsafe {
        mmd_runtime_instance_evaluate_clip_frame(descriptor_instance.0, clip.0, 0.0)
    });
    assert!(unsafe {
        mmd_runtime_instance_copy_world_matrices(
            pmx_instance.0,
            pmx_world.as_mut_ptr(),
            pmx_world.len(),
        )
    });
    assert!(unsafe {
        mmd_runtime_instance_copy_world_matrices(
            descriptor_instance.0,
            descriptor_world.as_mut_ptr(),
            descriptor_world.len(),
        )
    });
    assert_eq!(
        pmx_world, descriptor_world,
        "typed clip world matrix parity"
    );
    assert!(pmx_world.iter().any(|value| value.abs() > 1.0e-4));
    let enabled_world = descriptor_world.clone();
    let mut pmx_morphs = vec![0.0; parsed.morphs.len()];
    let mut descriptor_morphs = vec![0.0; parsed.morphs.len()];
    assert!(unsafe {
        mmd_runtime_instance_copy_morph_weights(
            pmx_instance.0,
            pmx_morphs.as_mut_ptr(),
            pmx_morphs.len(),
        )
    });
    assert!(unsafe {
        mmd_runtime_instance_copy_morph_weights(
            descriptor_instance.0,
            descriptor_morphs.as_mut_ptr(),
            descriptor_morphs.len(),
        )
    });
    assert_eq!(pmx_morphs, descriptor_morphs, "morph expansion parity");
    let group_offset = &group_morph_offsets[0];
    assert!(
        pmx_morphs[group_offset.child_morph_index as usize] > morph_keyframes[0].weight,
        "group morph must propagate to bone morph child"
    );

    let disabled_ik_clip = make_clip(
        &bone_keyframes,
        &morph_keyframes,
        &property_keyframes,
        &[0u8],
    );
    assert!(!disabled_ik_clip.0.is_null());
    assert!(unsafe {
        mmd_runtime_instance_evaluate_clip_frame(descriptor_instance.0, disabled_ik_clip.0, 0.0)
    });
    let mut disabled_ik_world = vec![0.0; matrix_len];
    assert!(unsafe {
        mmd_runtime_instance_copy_world_matrices(
            descriptor_instance.0,
            disabled_ik_world.as_mut_ptr(),
            disabled_ik_world.len(),
        )
    });
    let ik_bone = fixture.ik_solvers[0].ik_bone_index as usize;
    assert_ne!(
        &enabled_world[ik_bone * 16..ik_bone * 16 + 16],
        &disabled_ik_world[ik_bone * 16..ik_bone * 16 + 16],
        "IK enabled/disabled must change controller matrix"
    );

    let mut no_append_descriptor = fixture.descriptor();
    no_append_descriptor.append_transforms = ptr::null();
    no_append_descriptor.append_transform_count = 0;
    let no_append_model = DescriptorParityModel(unsafe {
        mmd_runtime_model_create_from_descriptor(&no_append_descriptor)
    });
    let no_append_instance =
        DescriptorParityInstance(unsafe { mmd_runtime_instance_create(no_append_model.0, 0) });
    assert!(!no_append_model.0.is_null());
    assert!(!no_append_instance.0.is_null());
    assert!(unsafe { mmd_runtime_instance_evaluate_clip_frame(no_append_instance.0, clip.0, 0.0) });
    let mut no_append_world = vec![0.0; matrix_len];
    assert!(unsafe {
        mmd_runtime_instance_copy_world_matrices(
            no_append_instance.0,
            no_append_world.as_mut_ptr(),
            no_append_world.len(),
        )
    });
    let append_target = fixture.append_transforms[0].target_bone_index as usize;
    assert_ne!(
        &enabled_world[append_target * 16..append_target * 16 + 16],
        &no_append_world[append_target * 16..append_target * 16 + 16],
        "append source input must affect target matrix"
    );

    let mut zero_morph_keyframes = morph_keyframes;
    zero_morph_keyframes[0].weight = 0.0;
    zero_morph_keyframes[1].weight = 0.0;
    let zero_morph_clip = make_clip(
        &bone_keyframes,
        &zero_morph_keyframes,
        &property_keyframes,
        &property_ik_enabled,
    );
    assert!(!zero_morph_clip.0.is_null());
    assert!(unsafe {
        mmd_runtime_instance_evaluate_clip_frame(descriptor_instance.0, zero_morph_clip.0, 0.0)
    });
    let mut zero_morph_world = vec![0.0; matrix_len];
    assert!(unsafe {
        mmd_runtime_instance_copy_world_matrices(
            descriptor_instance.0,
            zero_morph_world.as_mut_ptr(),
            zero_morph_world.len(),
        )
    });
    let morph_target = bone_morph_offsets[0].target_bone_index as usize;
    assert_ne!(
        &enabled_world[morph_target * 16..morph_target * 16 + 16],
        &zero_morph_world[morph_target * 16..morph_target * 16 + 16],
        "bone morph zero/nonzero must change target matrix"
    );

    let host_positions = [0.02, 0.0, 0.0, 0.0, -0.03, 0.0, 0.05, 0.0, 0.0];
    let host_rotations = [
        0.0, 0.0, 0.0, 1.0, // root
        0.0, 0.0, 0.0, 1.0, // append_after
        0.0, 0.0, 0.0, 1.0, // ik_controller
    ];
    let host_scales = [1.0; 9];
    let host_morph_weights = [0.0; 2];
    let host_ik_enabled = [1u8];
    let host_view = MmdRuntimeFfiHostPoseView {
        local_position_offsets_xyz: host_positions.as_ptr(),
        local_rotation_xyzw: host_rotations.as_ptr(),
        local_scales_xyz: host_scales.as_ptr(),
        bone_count: bones.len(),
        morph_weights: host_morph_weights.as_ptr(),
        morph_count: host_morph_weights.len(),
        ik_enabled: host_ik_enabled.as_ptr(),
        ik_count: host_ik_enabled.len(),
    };
    assert_eq!(
        unsafe {
            mmd_runtime_instance_apply_host_pose_and_evaluate_before_physics(
                pmx_instance.0,
                &host_view,
            )
        },
        MmdRuntimeStatus::Ok
    );
    assert_eq!(
        unsafe {
            mmd_runtime_instance_apply_host_pose_and_evaluate_before_physics(
                descriptor_instance.0,
                &host_view,
            )
        },
        MmdRuntimeStatus::Ok
    );
    assert!(unsafe {
        mmd_runtime_instance_copy_world_matrices(
            pmx_instance.0,
            pmx_world.as_mut_ptr(),
            pmx_world.len(),
        )
    });
    assert!(unsafe {
        mmd_runtime_instance_copy_world_matrices(
            descriptor_instance.0,
            descriptor_world.as_mut_ptr(),
            descriptor_world.len(),
        )
    });
    assert_eq!(pmx_world, descriptor_world, "host pose world matrix parity");
}

#[cfg(feature = "physics-bullet-native")]
#[test]
fn model_descriptor_fixture_physics_reset_and_steps_are_deterministic() {
    let bytes: &[u8] =
        include_bytes!("../../mmd-anim-format/fixtures/pmx/model_descriptor_parity.pmx");
    let fixture = OwnedDescriptorParityFixture::from_tracked_pmx();
    fixture.assert_shape();
    let descriptor = fixture.descriptor();
    let bones = &fixture.bones;

    let run = || {
        let pmx_model = DescriptorParityModel(unsafe {
            mmd_runtime_model_create_from_pmx_bytes(bytes.as_ptr(), bytes.len())
        });
        let descriptor_model =
            DescriptorParityModel(unsafe { mmd_runtime_model_create_from_descriptor(&descriptor) });
        assert!(!pmx_model.0.is_null());
        assert!(!descriptor_model.0.is_null());
        let pmx_instance =
            DescriptorParityInstance(unsafe { mmd_runtime_instance_create(pmx_model.0, 0) });
        let descriptor_instance =
            DescriptorParityInstance(unsafe { mmd_runtime_instance_create(descriptor_model.0, 0) });
        assert!(unsafe { mmd_runtime_instance_evaluate_rest_pose(pmx_instance.0) });
        assert!(unsafe { mmd_runtime_instance_evaluate_rest_pose(descriptor_instance.0) });

        let static_body = MmdRuntimeFfiPhysicsRigidBodyDesc {
            bone_index: 0,
            position_xyz: [0.0, 0.0, 0.0],
            mode: MmdRuntimeFfiPhysicsRigidBodyMode::Static as u32,
            body_from_bone_position_xyz: [10.0, 0.0, 0.0],
            ..static_physics_body_desc_with_nonzero_input_mass()
        };
        let dynamic_body = MmdRuntimeFfiPhysicsRigidBodyDesc {
            bone_index: 1,
            position_xyz: [0.0, 8.0, 0.0],
            mode: MmdRuntimeFfiPhysicsRigidBodyMode::Dynamic as u32,
            ..dynamic_physics_body_desc()
        };
        let rigidbodies = [static_body, dynamic_body];
        let joints: [MmdRuntimeFfiPhysicsJointDesc; 0] = [];
        let mut pmx_world = ptr::null_mut();
        let mut descriptor_world = ptr::null_mut();
        assert_eq!(
            unsafe {
                mmd_runtime_physics_world_create(
                    rigidbodies.as_ptr(),
                    rigidbodies.len(),
                    joints.as_ptr(),
                    joints.len(),
                    &mut pmx_world,
                )
            },
            MmdRuntimeStatus::Ok
        );
        let pmx_world = DescriptorParityWorld(pmx_world);
        assert_eq!(
            unsafe {
                mmd_runtime_physics_world_create(
                    rigidbodies.as_ptr(),
                    rigidbodies.len(),
                    joints.as_ptr(),
                    joints.len(),
                    &mut descriptor_world,
                )
            },
            MmdRuntimeStatus::Ok
        );
        let descriptor_world = DescriptorParityWorld(descriptor_world);
        assert!(!pmx_world.0.is_null());
        assert!(!descriptor_world.0.is_null());
        assert_eq!(
            unsafe {
                mmd_runtime_instance_set_physics_mode(
                    pmx_instance.0,
                    MmdRuntimeFfiPhysicsMode::Live as u32,
                )
            },
            MmdRuntimeStatus::Ok
        );
        assert_eq!(
            unsafe {
                mmd_runtime_instance_set_physics_mode(
                    descriptor_instance.0,
                    MmdRuntimeFfiPhysicsMode::Live as u32,
                )
            },
            MmdRuntimeStatus::Ok
        );
        let mut seeded_pmx = usize::MAX;
        let mut seeded_descriptor = usize::MAX;
        assert_eq!(
            unsafe {
                mmd_runtime_physics_world_reset(pmx_world.0, pmx_instance.0, &mut seeded_pmx)
            },
            MmdRuntimeStatus::Ok
        );
        assert_eq!(
            unsafe {
                mmd_runtime_physics_world_reset(
                    descriptor_world.0,
                    descriptor_instance.0,
                    &mut seeded_descriptor,
                )
            },
            MmdRuntimeStatus::Ok
        );
        assert!(seeded_pmx > 0);
        assert_eq!(seeded_pmx, seeded_descriptor);

        let mut baseline_states = vec![0.0; rigidbodies.len() * 7];
        assert_eq!(
            unsafe {
                mmd_runtime_physics_world_copy_rigidbody_states(
                    pmx_world.0,
                    baseline_states.as_mut_ptr(),
                    baseline_states.len(),
                )
            },
            MmdRuntimeStatus::Ok
        );
        let mut baseline = vec![0.0; bones.len() * 16];
        assert!(unsafe {
            mmd_runtime_instance_copy_world_matrices(
                pmx_instance.0,
                baseline.as_mut_ptr(),
                baseline.len(),
            )
        });
        assert_near(baseline[16 + 13], baseline_states[8], 1.0e-4);
        let mut checkpoints = Vec::new();
        let mut observed_step = false;
        for step in 0..30 {
            let mut pmx_report = zero_physics_step_report();
            let mut descriptor_report = zero_physics_step_report();
            assert_eq!(
                unsafe {
                    mmd_runtime_physics_world_step_runtime(
                        pmx_world.0,
                        pmx_instance.0,
                        1.0 / 60.0,
                        &mut pmx_report,
                    )
                },
                MmdRuntimeStatus::Ok
            );
            assert_eq!(
                unsafe {
                    mmd_runtime_physics_world_step_runtime(
                        descriptor_world.0,
                        descriptor_instance.0,
                        1.0 / 60.0,
                        &mut descriptor_report,
                    )
                },
                MmdRuntimeStatus::Ok
            );
            assert_eq!(
                pmx_report, descriptor_report,
                "physics report {step} parity"
            );
            assert_eq!(pmx_report.tick.substeps, 2);
            assert_eq!(pmx_report.kinematic_rigidbodies_fed, 1);
            assert_eq!(pmx_report.bones_written_back, 1);
            observed_step |= pmx_report.tick.substeps > 0 || pmx_report.bones_written_back > 0;
            if step == 9 || step == 29 {
                let mut pmx = vec![0.0; bones.len() * 16];
                let mut descriptor = vec![0.0; bones.len() * 16];
                let mut pmx_states = vec![0.0; rigidbodies.len() * 7];
                let mut descriptor_states = vec![0.0; rigidbodies.len() * 7];
                assert!(unsafe {
                    mmd_runtime_instance_copy_world_matrices(
                        pmx_instance.0,
                        pmx.as_mut_ptr(),
                        pmx.len(),
                    )
                });
                assert!(unsafe {
                    mmd_runtime_instance_copy_world_matrices(
                        descriptor_instance.0,
                        descriptor.as_mut_ptr(),
                        descriptor.len(),
                    )
                });
                assert_eq!(
                    unsafe {
                        mmd_runtime_physics_world_copy_rigidbody_states(
                            pmx_world.0,
                            pmx_states.as_mut_ptr(),
                            pmx_states.len(),
                        )
                    },
                    MmdRuntimeStatus::Ok
                );
                assert_eq!(
                    unsafe {
                        mmd_runtime_physics_world_copy_rigidbody_states(
                            descriptor_world.0,
                            descriptor_states.as_mut_ptr(),
                            descriptor_states.len(),
                        )
                    },
                    MmdRuntimeStatus::Ok
                );
                assert_eq!(pmx, descriptor, "physics checkpoint {step} parity");
                assert_eq!(
                    pmx_states, descriptor_states,
                    "physics rigidbody checkpoint {step} parity"
                );
                assert_near(pmx[16 + 13], pmx_states[8], 1.0e-4);
                if step == 29 {
                    assert_ne!(
                        pmx_states, baseline_states,
                        "dynamic rigidbody must advance after 30 steps"
                    );
                    assert!(
                        pmx_states[8] < baseline_states[8] - 1.0e-4,
                        "dynamic rigidbody y must fall: baseline={baseline_states:?} current={pmx_states:?}"
                    );
                    assert!(
                        pmx[16 + 13] < baseline[16 + 13] - 1.0e-4,
                        "after-physics bone y must receive dynamic readback"
                    );
                }
                checkpoints.push(pmx);
            }
        }
        assert!(
            observed_step,
            "physics step must advance or write back state"
        );
        checkpoints
    };

    let first = run();
    let second = run();
    assert_eq!(first, second, "fresh create/reset physics determinism");
    assert_eq!(first.len(), 2);
}
