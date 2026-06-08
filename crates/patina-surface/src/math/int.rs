//! i128-safe integer utilities for slab geometry and lattice arithmetic.
//!
//! Imported from `to_integrate_project/crystal_surface_generator/src/math/int.rs`
//! and kept as an inner scientific-kernel module.

use std::io::{Error, ErrorKind, Result};

/// Compact tuple alias for exact integer vectors.
pub type IVec3 = (i128, i128, i128);

fn gcd_u128(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r;
    }
    a
}

pub fn gcd_i128(a: i128, b: i128) -> i128 {
    let g = gcd_u128(a.unsigned_abs(), b.unsigned_abs());
    i128::try_from(g).expect("gcd exceeds i128::MAX")
}

pub fn gcd3_i128(a: i128, b: i128, c: i128) -> i128 {
    gcd_i128(gcd_i128(a, b), c)
}

pub fn dot_i128(a: IVec3, b: IVec3) -> i128 {
    a.0 * b.0 + a.1 * b.1 + a.2 * b.2
}

pub fn cross_i128(a: IVec3, b: IVec3) -> IVec3 {
    (
        a.1 * b.2 - a.2 * b.1,
        a.2 * b.0 - a.0 * b.2,
        a.0 * b.1 - a.1 * b.0,
    )
}

pub fn primitive_vec3_i128(v: IVec3) -> IVec3 {
    let g = gcd3_i128(v.0, v.1, v.2);
    if g == 0 {
        v
    } else {
        (v.0 / g, v.1 / g, v.2 / g)
    }
}

pub fn try_normalise_hkl_i128(h: i128, k: i128, l: i128) -> Option<IVec3> {
    let g = gcd3_i128(h, k, l);
    if g == 0 {
        None
    } else {
        Some((h / g, k / g, l / g))
    }
}

pub fn canonicalise_hkl_i128(h: i128, k: i128, l: i128) -> Option<IVec3> {
    let (mut h, mut k, mut l) = try_normalise_hkl_i128(h, k, l)?;

    let flip = h < 0 || (h == 0 && k < 0) || (h == 0 && k == 0 && l < 0);
    if flip {
        h = -h;
        k = -k;
        l = -l;
    }

    Some((h, k, l))
}

pub fn extended_gcd_i128(a: i128, b: i128) -> (i128, i128, i128) {
    if b == 0 {
        let g = i128::try_from(a.unsigned_abs()).expect("gcd exceeds i128::MAX");
        let x = if a == 0 { 0 } else { a.signum() };
        (g, x, 0)
    } else {
        let q = a.div_euclid(b);
        let r = a.rem_euclid(b);
        let (g, x1, y1) = extended_gcd_i128(b, r);
        (g, y1, x1 - q * y1)
    }
}

pub fn checked_i32(x: i128, ctx: &'static str) -> Result<i32> {
    i32::try_from(x)
        .map_err(|_| Error::new(ErrorKind::InvalidData, format!("{ctx} overflowed i32: {x}")))
}

pub fn saturating_i32(x: i128) -> i32 {
    if x > i32::MAX as i128 {
        i32::MAX
    } else if x < i32::MIN as i128 {
        i32::MIN
    } else {
        x as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gcd_i128_basic() {
        assert_eq!(gcd_i128(48, 18), 6);
        assert_eq!(gcd_i128(-48, 18), 6);
        assert_eq!(gcd_i128(48, -18), 6);
        assert_eq!(gcd_i128(0, 5), 5);
        assert_eq!(gcd_i128(0, 0), 0);
    }

    #[test]
    fn test_canonicalise_hkl_i128() {
        assert_eq!(canonicalise_hkl_i128(2, 4, 6), Some((1, 2, 3)));
        assert_eq!(canonicalise_hkl_i128(-2, -4, -6), Some((1, 2, 3)));
        assert_eq!(canonicalise_hkl_i128(0, -2, 4), Some((0, 1, -2)));
        assert_eq!(canonicalise_hkl_i128(0, 0, 0), None);
    }

    #[test]
    fn test_extended_gcd_i128_signed_cases() {
        let cases = [
            (48, 18),
            (-48, 18),
            (48, -18),
            (-48, -18),
            (0, 5),
            (5, 0),
            (-7, 3),
            (7, -3),
        ];

        for (a, b) in cases {
            let (g, x, y) = extended_gcd_i128(a, b);
            assert_eq!(a * x + b * y, g);
            assert_eq!(g, gcd_i128(a, b));
        }
    }

    #[test]
    fn test_primitive_vec3_i128() {
        assert_eq!(primitive_vec3_i128((2, 4, 6)), (1, 2, 3));
        assert_eq!(primitive_vec3_i128((0, 0, 0)), (0, 0, 0));
    }
}
