//! `ShortWeierstrassProjectivePoint<StarkCurve>`, flattened from lambdaworks-math 0.13.0
//! (elliptic_curve/short_weierstrass/point.rs, elliptic_curve/point.rs,
//! elliptic_curve/short_weierstrass/curves/stark_curve.rs).
//!
//! Projective coordinates [x : y : z] on y^2 z = x^3 + a x z^2 + b z^3; infinity is (0, 1, 0).

use crate::field::Fe;

/// StarkCurve `a` coefficient (= 1).
const A: Fe = Fe::from_hex_unchecked("1");

#[derive(Clone, Debug)]
pub struct Point {
    value: [Fe; 3],
}

impl Point {
    /// `ShortWeierstrassProjectivePoint::new_unchecked`.
    pub const fn new_unchecked(value: [Fe; 3]) -> Self {
        Point { value }
    }

    /// `StarkCurve::from_affine_hex_string_unchecked`.
    pub const fn from_affine_hex_unchecked(x_hex: &str, y_hex: &str) -> Self {
        Point::new_unchecked([
            Fe::from_hex_unchecked(x_hex),
            Fe::from_hex_unchecked(y_hex),
            Fe::from_hex_unchecked("1"),
        ])
    }

    pub fn x(&self) -> &Fe {
        &self.value[0]
    }

    pub fn coordinates(&self) -> &[Fe; 3] {
        &self.value
    }

    fn is_neutral_element(&self) -> bool {
        self.value[2] == Fe::ZERO
    }

    /// Returns [x/z : y/z : 1]; infinity maps to (0, 1, 0).
    pub fn to_affine(&self) -> Self {
        let [x, y, z] = self.coordinates();
        if z == &Fe::ZERO {
            return Self::new_unchecked([Fe::ZERO, Fe::ONE, Fe::ZERO]);
        };
        let inv_z = z.inv().unwrap();
        Point::new_unchecked([x * &inv_z, y * inv_z, Fe::ONE])
    }

    /// Doubling in projective coordinates (fallback of `operate_with_affine` when P == Q).
    pub fn double(&self) -> Self {
        if self.is_neutral_element() {
            return self.clone();
        }
        let [px, py, pz] = self.coordinates();

        let px_square = px * px;
        let three_px_square = &px_square + &px_square + &px_square;
        let w = A * pz * pz + three_px_square;
        let w_square = &w * &w;

        let s = py * pz;
        let s_square = &s * &s;
        let s_cube = &s * &s_square;
        let two_s_cube = &s_cube + &s_cube;
        let four_s_cube = &two_s_cube + &two_s_cube;
        let eight_s_cube = &four_s_cube + &four_s_cube;

        let b = px * py * &s;
        let two_b = &b + &b;
        let four_b = &two_b + &two_b;
        let eight_b = &four_b + &four_b;

        let h = &w_square - eight_b;
        let hs = &h * &s;

        let pys_square = py * py * s_square;
        let two_pys_square = &pys_square + &pys_square;
        let four_pys_square = &two_pys_square + &two_pys_square;
        let eight_pys_square = &four_pys_square + &four_pys_square;

        let xp = &hs + &hs;
        let yp = w * (four_b - &h) - eight_pys_square;
        let zp = eight_s_cube;

        Self::new_unchecked([xp, yp, zp])
    }

    /// Mixed addition, `other` must be affine (z = 1).
    /// https://hyperelliptic.org/EFD/g1p/data/shortw/projective/addition/madd-1998-cmo
    pub fn operate_with_affine(&self, other: &Self) -> Self {
        if self.is_neutral_element() {
            return other.clone();
        }
        if other.is_neutral_element() {
            return self.clone();
        }

        let [px, py, pz] = self.coordinates();
        let [qx, qy, _qz] = other.coordinates();
        let u = qy * pz;
        let v = qx * pz;

        if u == *py {
            if v != *px || *py == Fe::ZERO {
                return Self::new_unchecked([Fe::ZERO, Fe::ONE, Fe::ZERO]);
            } else {
                return self.double();
            }
        }

        let u = &u - py;
        let v = &v - px;
        let vv = &v * &v;
        let uu = &u * &u;
        let vvv = &v * &vv;
        let r = &vv * px;
        let a = &uu * pz - &vvv - &r - &r;

        let x = &v * &a;
        let y = &u * (&r - &a) - &vvv * py;
        let z = &vvv * pz;

        Self::new_unchecked([x, y, z])
    }
}
