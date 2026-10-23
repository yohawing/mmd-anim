//! Scalar rotation operations used by MikuDanceStudio's bone transform code.
//!
//! Preserve the D3DX operation order: tiny rounding changes can accumulate over
//! hundreds of CCD updates. These helpers deliberately do not normalize their
//! inputs or outputs, matching the reference operations.

use glam::{Mat4, Quat};

/// Round the Euler decomposition angle after evaluating atan2 in f64.
/// The Windows atan2f result for a traced CCD step rounds one f32 ULP above
/// Linux atan2f. That difference changes subsequent limit reflections. Keep
/// the inputs (including the preceding divisions) in f32, and only promote
/// this operation to preserve that Windows rounding on both platforms.
pub(crate) fn mmd_atan2(y: f32, x: f32) -> f32 {
    (y as f64).atan2(x as f64) as f32
}

// D3DX axis rotations evaluate trigonometry in double precision before storing
// f32 matrix entries. Using sinf/cosf changes the first YYB knee divergence by
// one ULP and subsequently changes the CCD stopping iteration.
pub(crate) fn mmd_axis_rotation(axis: usize, angle: f32) -> Mat4 {
    let s = (angle as f64).sin() as f32;
    let c = (angle as f64).cos() as f32;
    match axis {
        0 => Mat4::from_cols_array(&[1., 0., 0., 0., 0., c, s, 0., 0., -s, c, 0., 0., 0., 0., 1.]),
        1 => Mat4::from_cols_array(&[c, 0., -s, 0., 0., 1., 0., 0., s, 0., c, 0., 0., 0., 0., 1.]),
        2 => Mat4::from_cols_array(&[c, s, 0., 0., -s, c, 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.]),
        _ => unreachable!("rotation axis must be X, Y, or Z"),
    }
}

pub(crate) fn mmd_rotation_matrix(q: Quat) -> Mat4 {
    let [x, y, z, w] = q.to_array();
    let xx = x * (x + x);
    let yy = y * (y + y);
    let zz = z * (z + z);
    let xy = x * (y + y);
    let xz = x * (z + z);
    let yz = y * (z + z);
    let wx = w * (x + x);
    let wy = w * (y + y);
    let wz = w * (z + z);
    Mat4::from_cols_array(&[
        (1. - yy) - zz,
        xy + wz,
        xz - wy,
        0.,
        xy - wz,
        (1. - xx) - zz,
        yz + wx,
        0.,
        xz + wy,
        yz - wx,
        (1. - xx) - yy,
        0.,
        0.,
        0.,
        0.,
        1.,
    ])
}
/// D3DX argument convention: returns `b * a`.
pub(crate) fn mmd_quaternion_multiply(a: Quat, b: Quat) -> Quat {
    Quat::from_xyzw(
        ((b.y * a.z + b.x * a.w) - b.z * a.y) + b.w * a.x,
        ((b.y * a.w - b.x * a.z) + b.z * a.x) + b.w * a.y,
        ((-b.y * a.x + b.x * a.y) + b.z * a.w) + b.w * a.z,
        ((-b.y * a.y - b.x * a.x) - b.z * a.z) + b.w * a.w,
    )
}

pub(crate) fn mmd_quaternion_from_matrix(matrix: Mat4) -> Quat {
    let m = glam::Mat3::from_mat4(matrix).to_cols_array_2d();
    let trace = m[0][0] + m[1][1] + m[2][2];
    let mut q = [0.0; 4];
    if trace >= 0.0 {
        let s = (trace + 1.0).sqrt();
        q[3] = s * 0.5;
        let scale = 0.5 / s;
        q[0] = (m[1][2] - m[2][1]) * scale;
        q[1] = (m[2][0] - m[0][2]) * scale;
        q[2] = (m[0][1] - m[1][0]) * scale;
    } else {
        let mut i = 0;
        if m[1][1] > m[0][0] {
            i = 1;
        }
        if m[2][2] > m[i][i] {
            i = 2;
        }
        let j = (i + 1) % 3;
        let k = (j + 1) % 3;
        let s = (m[i][i] - m[j][j] - m[k][k] + 1.0).sqrt();
        q[i] = s * 0.5;
        let scale = 0.5 / s;
        q[3] = (m[j][k] - m[k][j]) * scale;
        q[j] = (m[i][j] + m[j][i]) * scale;
        q[k] = (m[i][k] + m[k][i]) * scale;
    }
    Quat::from_array(q)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn euler_atan2_preserves_windows_limited_chain_rounding() {
        // limited-n1, second CCD iteration, distal link: identical candidate
        // matrices on Windows/Linux previously produced different Euler Z.
        let y = f32::from_bits(0x3f68ab27);
        let x = f32::from_bits(0x3ed58dde);
        assert_eq!(mmd_atan2(y, x).to_bits(), 0x3f91fd6d);
    }

    #[test]
    fn axis_rotation_preserves_d3dx_knee_recomposition_rounding() {
        // Native D3DX9_43 capture: iteration 7 of a limited knee. The input
        // quaternion and Euler angle already agree; sinf rounds one ULP away
        // from D3DX here and changes subsequent CCD convergence.
        let angle = f32::from_bits(0xbf33ba29);
        let sine = f32::from_bits(0xbf255295);
        let cosine = f32::from_bits(0x3f4375a6);
        let matrix = mmd_axis_rotation(0, angle);
        assert_eq!(matrix.y_axis.y.to_bits(), cosine.to_bits());
        assert_eq!(matrix.y_axis.z.to_bits(), sine.to_bits());
        assert_eq!(matrix.z_axis.y.to_bits(), (-sine).to_bits());
        assert_eq!(
            mmd_axis_rotation(1, angle).z_axis.x.to_bits(),
            sine.to_bits()
        );
        assert_eq!(
            mmd_axis_rotation(2, angle).x_axis.y.to_bits(),
            sine.to_bits()
        );
        let rotation = mmd_quaternion_from_matrix(matrix);
        assert_eq!(rotation.x.to_bits(), 0xbeb00f0e);
        assert_eq!(rotation.w.to_bits(), 0x3f70638f);
    }
}
