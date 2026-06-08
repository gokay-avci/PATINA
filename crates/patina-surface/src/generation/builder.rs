use crate::domain::{array_to_matrix3, SurfaceGenerationConfig, SurfaceInterfaceError};
use crate::math::int::gcd_i128;
use crate::math::integer_basis::find_primitive_in_plane_basis;
use crate::math::lll::{lll_reduce, reduce_2d_integer};
use nalgebra::{Matrix3, Vector3};

#[derive(Debug, Clone)]
pub(crate) struct LatticeOps {
    pub(crate) matrix: Matrix3<f64>,
    pub(crate) reciprocal_matrix: Matrix3<f64>,
}

impl LatticeOps {
    pub(crate) fn new(lattice: [[f64; 3]; 3]) -> Result<Self, SurfaceInterfaceError> {
        let matrix = array_to_matrix3(lattice);
        if matrix.determinant().abs() < 1.0e-8 {
            return Err(SurfaceInterfaceError::DegenerateLattice);
        }
        let reciprocal_matrix = matrix
            .try_inverse()
            .ok_or(SurfaceInterfaceError::NonInvertibleLattice)?
            .transpose();
        Ok(Self {
            matrix,
            reciprocal_matrix,
        })
    }

    pub(crate) fn to_cartesian(&self, fractional: [f64; 3]) -> Vector3<f64> {
        self.matrix * Vector3::new(fractional[0], fractional[1], fractional[2])
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SlabGeometry {
    pub(crate) basis: Matrix3<f64>,
    pub(crate) d_hkl: f64,
    pub(crate) n_layers: usize,
    pub(crate) slab_normal: Vector3<f64>,
}

pub(crate) fn compute_geometry(
    lattice: &LatticeOps,
    config: &SurfaceGenerationConfig,
    warnings: &mut Vec<String>,
) -> Result<SlabGeometry, SurfaceInterfaceError> {
    let (h, k, l) = normalized_hkl(config.miller.as_array())?;
    let (u_raw, v_raw) = find_primitive_in_plane_basis(h, k, l)
        .map_err(|err| SurfaceInterfaceError::MathKernel(err.to_string()))?;
    let (u_int, v_int) = reduce_2d_integer(u_raw, v_raw);

    let u_cart = lattice.matrix * Vector3::new(u_int[0] as f64, u_int[1] as f64, u_int[2] as f64);
    let v_cart = lattice.matrix * Vector3::new(v_int[0] as f64, v_int[1] as f64, v_int[2] as f64);

    let len_u = u_cart.norm();
    let len_v = v_cart.norm();
    let ratio = if len_u > len_v {
        len_u / len_v.max(1.0e-12)
    } else {
        len_v / len_u.max(1.0e-12)
    };
    if ratio > 5.0 {
        warnings.push(format!(
            "high in-plane aspect ratio {:.2} detected for surface ({h} {k} {l})",
            ratio
        ));
    }

    let reciprocal_n = lattice.reciprocal_matrix * Vector3::new(h as f64, k as f64, l as f64);
    let g_norm = reciprocal_n.norm();
    if g_norm < 1.0e-12 {
        return Err(SurfaceInterfaceError::DegenerateSurfaceNormal);
    }
    let d_hkl = 1.0 / g_norm;
    let n_layers = (config.thickness_angstrom / d_hkl).round().max(1.0) as usize;
    let slab_height = n_layers as f64 * d_hkl;
    let slab_normal = reciprocal_n.normalize();
    let c_slab = slab_normal * (slab_height + config.vacuum_angstrom);

    let temp_basis = Matrix3::from_columns(&[u_cart, v_cart, slab_normal * 10_000.0]);
    let reduced = lll_reduce(temp_basis);
    let basis = Matrix3::from_columns(&[
        reduced.column(0).into_owned(),
        reduced.column(1).into_owned(),
        c_slab,
    ]);

    Ok(SlabGeometry {
        basis,
        d_hkl,
        n_layers,
        slab_normal,
    })
}

pub(crate) fn normalized_hkl(miller: [i32; 3]) -> Result<(i32, i32, i32), SurfaceInterfaceError> {
    let (h, k, l) = (miller[0] as i128, miller[1] as i128, miller[2] as i128);
    if h == 0 && k == 0 && l == 0 {
        return Err(SurfaceInterfaceError::ZeroMillerIndex);
    }
    let g = gcd_i128(gcd_i128(h, k), l).max(1);
    Ok(((h / g) as i32, (k / g) as i32, (l / g) as i32))
}
