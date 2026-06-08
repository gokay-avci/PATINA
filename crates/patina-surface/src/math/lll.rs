//! LLL and 2D integer reduction helpers for slab basis construction.
//!
//! Imported from `to_integrate_project/crystal_surface_generator/src/math/lll.rs`.

use nalgebra::{Matrix3, Vector3};

use crate::math::int::{checked_i32, saturating_i32};

const LLL_DELTA: f64 = 0.75;
const GS_EPS: f64 = 1.0e-14;

type IVec3 = (i128, i128, i128);

#[inline]
fn tuple_dot_i128(a: IVec3, b: IVec3) -> i128 {
    a.0 * b.0 + a.1 * b.1 + a.2 * b.2
}

#[inline]
fn tuple_sub_i128(a: IVec3, b: IVec3) -> IVec3 {
    (a.0 - b.0, a.1 - b.1, a.2 - b.2)
}

#[inline]
fn tuple_scale_i128(a: IVec3, k: i128) -> IVec3 {
    (a.0 * k, a.1 * k, a.2 * k)
}

#[inline]
fn tuple_vec3_i128_to_i32_saturating(v: IVec3) -> Vector3<i32> {
    Vector3::new(
        saturating_i32(v.0),
        saturating_i32(v.1),
        saturating_i32(v.2),
    )
}

#[inline]
fn tuple_vec3_i128_to_i32_checked(v: IVec3) -> Option<Vector3<i32>> {
    let x = checked_i32(v.0, "reduce_2d_integer.x").ok()?;
    let y = checked_i32(v.1, "reduce_2d_integer.y").ok()?;
    let z = checked_i32(v.2, "reduce_2d_integer.z").ok()?;
    Some(Vector3::new(x, y, z))
}

fn round_div_nearest_i128(num: i128, den: i128) -> i128 {
    debug_assert!(den > 0);

    if num >= 0 {
        (num + den / 2) / den
    } else {
        -((-num + den / 2) / den)
    }
}

fn gram_schmidt_columns(b: &Matrix3<f64>) -> ([Vector3<f64>; 3], [[f64; 3]; 3]) {
    let mut b_star = [
        b.column(0).into_owned(),
        b.column(1).into_owned(),
        b.column(2).into_owned(),
    ];
    let mut mu = [[0.0_f64; 3]; 3];

    for i in 0..3 {
        let mut v = b.column(i).into_owned();

        for j in 0..i {
            let denom = b_star[j].dot(&b_star[j]);
            if denom <= GS_EPS {
                mu[i][j] = 0.0;
                continue;
            }

            let coeff = b.column(i).dot(&b_star[j]) / denom;
            mu[i][j] = coeff;
            v -= b_star[j] * coeff;
        }

        b_star[i] = v;
    }

    (b_star, mu)
}

pub fn lll_reduce(basis: Matrix3<f64>) -> Matrix3<f64> {
    let mut b = basis;
    let n = 3usize;
    let mut k = 1usize;

    while k < n {
        let (_, mu_before) = gram_schmidt_columns(&b);

        for j in (0..k).rev() {
            let coeff = mu_before[k][j];
            if coeff.abs() > 0.5 {
                let r = coeff.round();
                let mut col_k = b.column(k).into_owned();
                col_k -= b.column(j) * r;
                b.set_column(k, &col_k);
            }
        }

        let (b_star, mu_after) = gram_schmidt_columns(&b);

        let denom_km1 = b_star[k - 1].dot(&b_star[k - 1]);
        let norm_k = b_star[k].dot(&b_star[k]);

        if denom_km1 <= GS_EPS {
            k += 1;
            continue;
        }

        let mu_k_km1 = mu_after[k][k - 1];
        let lovasz_rhs = (LLL_DELTA - mu_k_km1 * mu_k_km1) * denom_km1;

        if norm_k + GS_EPS < lovasz_rhs {
            b.swap_columns(k, k - 1);
            k = k.saturating_sub(1).max(1);
        } else {
            k += 1;
        }
    }

    b
}

pub fn reduce_2d_integer_i128(
    mut u: Vector3<i128>,
    mut v: Vector3<i128>,
) -> (Vector3<i128>, Vector3<i128>) {
    let to_t = |x: &Vector3<i128>| (x.x, x.y, x.z);

    if tuple_dot_i128(to_t(&u), to_t(&u)) > tuple_dot_i128(to_t(&v), to_t(&v)) {
        std::mem::swap(&mut u, &mut v);
    }

    loop {
        let uu = tuple_dot_i128(to_t(&u), to_t(&u));
        if uu == 0 {
            return (u, v);
        }

        let uv = tuple_dot_i128(to_t(&u), to_t(&v));
        let mu = round_div_nearest_i128(uv, uu);

        if mu == 0 {
            return (u, v);
        }

        let v_new_t = tuple_sub_i128(to_t(&v), tuple_scale_i128(to_t(&u), mu));
        let vv_old = tuple_dot_i128(to_t(&v), to_t(&v));
        let vv_new = tuple_dot_i128(v_new_t, v_new_t);

        if vv_new >= vv_old {
            return (u, v);
        }

        v = Vector3::new(v_new_t.0, v_new_t.1, v_new_t.2);

        if tuple_dot_i128(to_t(&u), to_t(&u)) > tuple_dot_i128(to_t(&v), to_t(&v)) {
            std::mem::swap(&mut u, &mut v);
        }
    }
}

pub fn reduce_2d_integer(u: Vector3<i32>, v: Vector3<i32>) -> (Vector3<i32>, Vector3<i32>) {
    let u128 = Vector3::new(u.x as i128, u.y as i128, u.z as i128);
    let v128 = Vector3::new(v.x as i128, v.y as i128, v.z as i128);

    let (u_red, v_red) = reduce_2d_integer_i128(u128, v128);

    let u_out = tuple_vec3_i128_to_i32_checked((u_red.x, u_red.y, u_red.z))
        .unwrap_or_else(|| tuple_vec3_i128_to_i32_saturating((u_red.x, u_red.y, u_red.z)));
    let v_out = tuple_vec3_i128_to_i32_checked((v_red.x, v_red.y, v_red.z))
        .unwrap_or_else(|| tuple_vec3_i128_to_i32_saturating((v_red.x, v_red.y, v_red.z)));

    (u_out, v_out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reduce_2d_already_reduced() {
        let u = Vector3::new(1, 0, 0);
        let v = Vector3::new(0, 1, 0);
        let (u_res, v_res) = reduce_2d_integer(u, v);
        assert_eq!(u_res, u);
        assert_eq!(v_res, v);
    }

    #[test]
    fn test_lll_preserves_rank_on_simple_basis() {
        let basis = Matrix3::from_columns(&[
            Vector3::new(3.0, 0.0, 0.0),
            Vector3::new(1.0, 2.0, 0.0),
            Vector3::new(0.0, 0.0, 5.0),
        ]);
        let reduced = lll_reduce(basis);
        assert!(reduced.determinant().abs() > 1.0e-8);
    }
}
