use glam::{Quat, Vec3A};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AppendPrimitiveInput {
    pub source_position_offset: Vec3A,
    pub source_rotation: Quat,
    pub ratio: f32,
    pub affect_rotation: bool,
    pub affect_translation: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AppendPrimitiveOutput {
    pub position_offset: Vec3A,
    pub rotation: Quat,
}

impl Default for AppendPrimitiveOutput {
    fn default() -> Self {
        Self {
            position_offset: Vec3A::ZERO,
            rotation: Quat::IDENTITY,
        }
    }
}

pub fn solve_append_transform(input: AppendPrimitiveInput) -> AppendPrimitiveOutput {
    let rotation = if input.affect_rotation {
        Quat::IDENTITY
            .slerp(input.source_rotation, input.ratio)
            .normalize()
    } else {
        Quat::IDENTITY
    };
    let position_offset = if input.affect_translation {
        input.source_position_offset * input.ratio
    } else {
        Vec3A::ZERO
    };

    AppendPrimitiveOutput {
        position_offset,
        rotation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_vec3a_near(actual: Vec3A, expected: Vec3A) {
        let delta = (actual - expected).abs();
        assert!(
            delta.x < 1.0e-5 && delta.y < 1.0e-5 && delta.z < 1.0e-5,
            "actual={actual:?} expected={expected:?} delta={delta:?}"
        );
    }

    #[test]
    fn solves_rotation_translation_and_ratio() {
        let output = solve_append_transform(AppendPrimitiveInput {
            source_position_offset: Vec3A::new(2.0, 4.0, -6.0),
            source_rotation: Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            ratio: 0.5,
            affect_rotation: true,
            affect_translation: true,
        });

        assert_vec3a_near(output.position_offset, Vec3A::new(1.0, 2.0, -3.0));
        assert_vec3a_near(
            output.rotation.mul_vec3a(Vec3A::X),
            Vec3A::new(
                std::f32::consts::FRAC_1_SQRT_2,
                std::f32::consts::FRAC_1_SQRT_2,
                0.0,
            ),
        );
    }

    #[test]
    fn disabled_channels_return_identity_offsets() {
        let output = solve_append_transform(AppendPrimitiveInput {
            source_position_offset: Vec3A::new(2.0, 0.0, 0.0),
            source_rotation: Quat::from_rotation_y(1.0),
            ratio: 1.0,
            affect_rotation: false,
            affect_translation: false,
        });

        assert_vec3a_near(output.position_offset, Vec3A::ZERO);
        assert_eq!(output.rotation, Quat::IDENTITY);
    }

    #[test]
    fn dependency_order_characterization_keeps_known_append_delta_bounded() {
        let source_local = Quat::from_rotation_z(0.8);
        let upstream = solve_append_transform(AppendPrimitiveInput {
            source_position_offset: Vec3A::new(2.0, 0.0, 0.0),
            source_rotation: source_local,
            ratio: 0.5,
            affect_rotation: true,
            affect_translation: true,
        });

        let correct_order = solve_append_transform(AppendPrimitiveInput {
            source_position_offset: upstream.position_offset,
            source_rotation: upstream.rotation,
            ratio: 0.5,
            affect_rotation: true,
            affect_translation: true,
        });
        let dependency_order = solve_append_transform(AppendPrimitiveInput {
            source_position_offset: Vec3A::new(2.0, 0.0, 0.0),
            source_rotation: source_local,
            ratio: 0.5,
            affect_rotation: true,
            affect_translation: true,
        });

        let position_delta =
            (correct_order.position_offset - dependency_order.position_offset).length();
        let angular_delta = correct_order
            .rotation
            .angle_between(dependency_order.rotation);

        assert!(
            position_delta > 0.0 && angular_delta > 0.0,
            "fixture must characterize a non-zero strict-order vs dependency-order delta"
        );
        assert!(
            position_delta <= 0.5 && angular_delta <= 0.21,
            "characterization budget widened unexpectedly: position_delta={position_delta} angular_delta={angular_delta}"
        );
    }

    #[test]
    fn append_primitive_is_bit_deterministic_in_current_process_profile() {
        // The workspace currently enables glam fast-math; this test claims bit
        // identity only for repeated solves in the same process/build profile.
        let input = AppendPrimitiveInput {
            source_position_offset: Vec3A::new(0.25, -0.5, 1.25),
            source_rotation: Quat::from_rotation_y(0.7),
            ratio: 0.375,
            affect_rotation: true,
            affect_translation: true,
        };
        let expected = solve_append_transform(input);
        for _ in 0..32 {
            let actual = solve_append_transform(input);
            assert_eq!(
                actual.position_offset.to_array().map(f32::to_bits),
                expected.position_offset.to_array().map(f32::to_bits)
            );
            assert_eq!(
                actual.rotation.to_array().map(f32::to_bits),
                expected.rotation.to_array().map(f32::to_bits)
            );
        }
    }
}
