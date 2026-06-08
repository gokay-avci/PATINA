pub(crate) mod bonding;
#[cfg(test)]
pub(crate) mod connectivity;
#[cfg(test)]
pub(crate) mod neighbours;
#[cfg(test)]
pub(crate) mod voronoi_approx;

use crate::domain::{array_to_matrix3, SurfaceSlab};
use nalgebra::{Matrix3, Vector3};

pub(crate) fn slab_lattice_matrix(slab: &SurfaceSlab) -> Matrix3<f64> {
    array_to_matrix3(slab.lattice)
}

#[cfg(test)]
pub(crate) fn slab_cartesian(slab: &SurfaceSlab, fractional: [f64; 3]) -> Vector3<f64> {
    slab_lattice_matrix(slab) * Vector3::new(fractional[0], fractional[1], fractional[2])
}

pub(crate) fn shortest_distance_vector(
    slab: &SurfaceSlab,
    left: [f64; 3],
    right: [f64; 3],
) -> Vector3<f64> {
    let mut delta = Vector3::new(right[0] - left[0], right[1] - left[1], right[2] - left[2]);
    for axis in 0..3 {
        if slab.periodic_axes[axis] {
            delta[axis] -= delta[axis].round();
        }
    }
    slab_lattice_matrix(slab) * delta
}

#[cfg(test)]
pub(crate) fn wrap_fractional_axis(value: f64, periodic: bool) -> f64 {
    if periodic {
        value.rem_euclid(1.0)
    } else {
        value
    }
}

#[cfg(test)]
pub(crate) fn wrap_index(index: i32, n: i32) -> i32 {
    ((index % n) + n) % n
}
