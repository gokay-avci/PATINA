//! Exact i128-safe basis construction for Miller-plane geometry.
//!
//! Imported from `to_integrate_project/crystal_surface_generator/src/math/integer_basis.rs`
//! and adapted to local error types.

use std::io::{Error, ErrorKind, Result};

use nalgebra::Vector3;

use crate::math::int::{
    canonicalise_hkl_i128, checked_i32, cross_i128, dot_i128, extended_gcd_i128, gcd_i128,
    primitive_vec3_i128, IVec3,
};

fn vec3_i128_to_i32_checked(v: IVec3, name: &'static str) -> Result<Vector3<i32>> {
    Ok(Vector3::new(
        checked_i32(
            v.0,
            match name {
                "u" => "u.x",
                "v" => "v.x",
                "w" => "w.x",
                _ => "x",
            },
        )?,
        checked_i32(
            v.1,
            match name {
                "u" => "u.y",
                "v" => "v.y",
                "w" => "w.y",
                _ => "y",
            },
        )?,
        checked_i32(
            v.2,
            match name {
                "u" => "u.z",
                "v" => "v.z",
                "w" => "w.z",
                _ => "z",
            },
        )?,
    ))
}

pub fn find_primitive_in_plane_basis_i128(
    h: i128,
    k: i128,
    l: i128,
) -> Result<(Vector3<i128>, Vector3<i128>)> {
    let n = canonicalise_hkl_i128(h, k, l)
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "Miller indices cannot be (0,0,0)."))?;

    let (hh, kk, ll) = n;

    let (u_t, v_t) = if hh == 0 && kk == 0 {
        ((1, 0, 0), (0, 1, 0))
    } else {
        let g = gcd_i128(hh, kk);
        let u = (kk / g, -hh / g, 0);
        let (g_check, p, q) = extended_gcd_i128(hh, kk);
        debug_assert_eq!(g_check, g);
        let v = (-ll * p, -ll * q, g);
        (u, v)
    };

    let u = primitive_vec3_i128(u_t);
    let mut v = v_t;

    if dot_i128(cross_i128(u, v), n) < 0 {
        v = (-v.0, -v.1, -v.2);
    }

    debug_assert_eq!(dot_i128(n, u), 0);
    debug_assert_eq!(dot_i128(n, v), 0);
    debug_assert_ne!(cross_i128(u, v), (0, 0, 0));

    Ok((Vector3::new(u.0, u.1, u.2), Vector3::new(v.0, v.1, v.2)))
}

pub fn find_primitive_in_plane_basis(
    h: i32,
    k: i32,
    l: i32,
) -> Result<(Vector3<i32>, Vector3<i32>)> {
    let (u, v) = find_primitive_in_plane_basis_i128(h as i128, k as i128, l as i128)?;

    let u32 = vec3_i128_to_i32_checked((u.x, u.y, u.z), "u")?;
    let v32 = vec3_i128_to_i32_checked((v.x, v.y, v.z), "v")?;

    Ok((u32, v32))
}

// The exact stacking-vector contract is retained in tests until the slab
// builder switches over to an explicit runtime layer-step path.
#[cfg(test)]
pub fn find_stacking_vector_i128(h: i128, k: i128, l: i128) -> Result<Vector3<i128>> {
    let n = canonicalise_hkl_i128(h, k, l)
        .ok_or_else(|| Error::new(ErrorKind::InvalidInput, "Miller indices cannot be (0,0,0)."))?;
    let (hh, kk, ll) = n;

    let (g12, x12, y12) = extended_gcd_i128(hh, kk);
    let (g123, s, t) = extended_gcd_i128(g12, ll);
    if g123 != 1 {
        return Err(Error::new(
            ErrorKind::InvalidData,
            format!("primitive HKL should have gcd 1, but got gcd {g123} for ({hh},{kk},{ll})"),
        ));
    }

    let x = x12 * s;
    let y = y12 * s;
    let z = t;

    let w = (x, y, z);
    debug_assert_eq!(dot_i128((hh, kk, ll), w), 1);

    Ok(Vector3::new(x, y, z))
}

#[cfg(test)]
pub fn find_stacking_vector_checked(h: i32, k: i32, l: i32) -> Result<Vector3<i32>> {
    let w = find_stacking_vector_i128(h as i128, k as i128, l as i128)?;
    vec3_i128_to_i32_checked((w.x, w.y, w.z), "w")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dot_i32(h: i32, k: i32, l: i32, v: &Vector3<i32>) -> i128 {
        (h as i128) * (v.x as i128) + (k as i128) * (v.y as i128) + (l as i128) * (v.z as i128)
    }

    #[test]
    fn test_find_primitive_in_plane_basis_axis_normal() {
        let (u, v) = find_primitive_in_plane_basis(0, 0, 1).unwrap();
        assert_eq!(u.dot(&Vector3::new(0, 0, 1)), 0);
        assert_eq!(v.dot(&Vector3::new(0, 0, 1)), 0);
        assert_ne!(u.cross(&v), Vector3::new(0, 0, 0));
    }

    #[test]
    fn test_find_primitive_in_plane_basis_111() {
        let (u, v) = find_primitive_in_plane_basis(1, 1, 1).unwrap();
        let n = Vector3::new(1, 1, 1);
        assert_eq!(u.dot(&n), 0);
        assert_eq!(v.dot(&n), 0);
        assert_ne!(u.cross(&v), Vector3::new(0, 0, 0));
    }

    #[test]
    fn test_find_primitive_in_plane_basis_negative_hkl() {
        let (u, v) = find_primitive_in_plane_basis(-1, 2, -3).unwrap();
        let n = Vector3::new(-1, 2, -3);
        assert_eq!(u.dot(&n), 0);
        assert_eq!(v.dot(&n), 0);
        assert_ne!(u.cross(&v), Vector3::new(0, 0, 0));
    }

    #[test]
    fn test_find_primitive_in_plane_basis_rejects_zero_hkl() {
        assert!(find_primitive_in_plane_basis(0, 0, 0).is_err());
    }

    #[test]
    fn test_find_stacking_vector_checked_contract() {
        let w = find_stacking_vector_checked(2, 4, 6).unwrap();
        assert_eq!(dot_i32(2, 4, 6, &w), 2);
    }

    #[test]
    fn test_find_stacking_vector_checked_primitive_contract() {
        let w = find_stacking_vector_checked(1, 1, 1).unwrap();
        assert_eq!(dot_i32(1, 1, 1, &w), 1);
    }
}
