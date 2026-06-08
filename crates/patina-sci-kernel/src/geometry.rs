use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GeometryRejectionReason {
    UnphysicalCell,
    Collapsed,
    Fragmented,
    ZeroCoordinated,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CellAssessment {
    pub rejection: Option<GeometryRejectionReason>,
    pub lengths: [f64; 3],
    pub angles_deg: [f64; 3],
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct UnitCellParameters {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub alpha_deg: f64,
    pub beta_deg: f64,
    pub gamma_deg: f64,
}

pub fn assess_periodic_cell(lattice: [[f64; 3]; 3]) -> CellAssessment {
    let lengths = [
        vector_norm(lattice[0]),
        vector_norm(lattice[1]),
        vector_norm(lattice[2]),
    ];
    let angles_deg = [
        angle_deg(lattice[1], lattice[2]),
        angle_deg(lattice[0], lattice[2]),
        angle_deg(lattice[0], lattice[1]),
    ];

    let has_unphysical_lengths = lengths
        .iter()
        .any(|value| !value.is_finite() || *value <= 0.0);
    let has_unphysical_angles = angles_deg
        .iter()
        .any(|value| !value.is_finite() || *value <= 0.0 || *value >= 180.0);
    let violates_angle_triangle = angles_deg[0] + angles_deg[1] + angles_deg[2] > 360.0
        || (angles_deg[0] - angles_deg[1]).abs() > angles_deg[2]
        || (angles_deg[2] - angles_deg[0]).abs() > angles_deg[1]
        || (angles_deg[1] - angles_deg[2]).abs() > angles_deg[0];
    let rejection = (has_unphysical_lengths || has_unphysical_angles || violates_angle_triangle)
        .then_some(GeometryRejectionReason::UnphysicalCell);

    CellAssessment {
        rejection,
        lengths,
        angles_deg,
    }
}

pub fn lattice_vectors_from_params(params: UnitCellParameters) -> [[f64; 3]; 3] {
    let alpha = params.alpha_deg.to_radians();
    let beta = params.beta_deg.to_radians();
    let gamma = params.gamma_deg.to_radians();

    let cos_alpha = if params.alpha_deg == 90.0 {
        0.0
    } else {
        alpha.cos()
    };
    let cos_beta = if params.beta_deg == 90.0 {
        0.0
    } else {
        beta.cos()
    };
    let (sin_gamma, cos_gamma) = if params.gamma_deg == 90.0 {
        (1.0, 0.0)
    } else {
        (gamma.sin(), gamma.cos())
    };

    let f = (cos_alpha - cos_beta * cos_gamma) / sin_gamma;
    let g = 1.0 - cos_beta * cos_beta - f * f;

    [
        [params.a, 0.0, 0.0],
        [params.b * cos_gamma, params.b * sin_gamma, 0.0],
        [
            params.c * cos_beta,
            params.c * f,
            if g < 1.0e-20 {
                0.0
            } else {
                params.c * g.sqrt()
            },
        ],
    ]
}

pub fn lattice_params_from_vectors(lattice: [[f64; 3]; 3]) -> UnitCellParameters {
    let a = vector_norm(lattice[0]);
    let b = vector_norm(lattice[1]);
    let c = vector_norm(lattice[2]);
    let alpha = angle_deg(lattice[1], lattice[2]);
    let beta = angle_deg(lattice[2], lattice[0]);
    let gamma = angle_deg(lattice[0], lattice[1]);

    UnitCellParameters {
        a,
        b,
        c,
        alpha_deg: snap_angle(alpha),
        beta_deg: snap_angle(beta),
        gamma_deg: snap_angle(gamma),
    }
}

pub fn fractional_to_cartesian(lattice: [[f64; 3]; 3], fractional: [f64; 3]) -> [f64; 3] {
    [
        lattice[0][0] * fractional[0]
            + lattice[1][0] * fractional[1]
            + lattice[2][0] * fractional[2],
        lattice[0][1] * fractional[0]
            + lattice[1][1] * fractional[1]
            + lattice[2][1] * fractional[2],
        lattice[0][2] * fractional[0]
            + lattice[1][2] * fractional[1]
            + lattice[2][2] * fractional[2],
    ]
}

pub fn cartesian_to_fractional(lattice: [[f64; 3]; 3], cartesian: [f64; 3]) -> Option<[f64; 3]> {
    let inv = invert_basis_matrix(lattice)?;
    Some(multiply_matrix_vector(inv, cartesian))
}

pub fn minimum_image_cartesian_distance_sq(
    left: [f64; 3],
    right: [f64; 3],
    lattice: Option<[[f64; 3]; 3]>,
) -> f64 {
    minimum_image_cartesian_distance_sq_with_axes(left, right, lattice, [true, true, true])
}

pub fn minimum_image_cartesian_distance_sq_with_axes(
    left: [f64; 3],
    right: [f64; 3],
    lattice: Option<[[f64; 3]; 3]>,
    periodic_axes: [bool; 3],
) -> f64 {
    if let Some(lattice) = lattice {
        if let Some(inv) = invert_basis_matrix(lattice) {
            let left_frac = multiply_matrix_vector(inv, left);
            let right_frac = multiply_matrix_vector(inv, right);
            let mut delta = [
                left_frac[0] - right_frac[0],
                left_frac[1] - right_frac[1],
                left_frac[2] - right_frac[2],
            ];
            for axis in 0..3 {
                if periodic_axes[axis] {
                    delta[axis] -= delta[axis].round();
                }
            }
            let cart = fractional_to_cartesian(lattice, delta);
            return cart[0] * cart[0] + cart[1] * cart[1] + cart[2] * cart[2];
        }
    }

    let dx = left[0] - right[0];
    let dy = left[1] - right[1];
    let dz = left[2] - right[2];
    dx * dx + dy * dy + dz * dz
}

pub fn normalize_fractional_coordinate(fractional: [f64; 3]) -> [f64; 3] {
    normalize_fractional_coordinate_with_axes(fractional, [true, true, true])
}

pub fn normalize_fractional_coordinate_with_axes(
    mut fractional: [f64; 3],
    periodic_axes: [bool; 3],
) -> [f64; 3] {
    for axis in 0..3 {
        if periodic_axes[axis] {
            fractional[axis] = fractional[axis].rem_euclid(1.0);
            if fractional[axis] >= 1.0 {
                fractional[axis] = 0.0;
            }
        }
    }
    fractional
}

pub fn nearest_periodic_image_fractional(
    anchor_fractional: [f64; 3],
    target_fractional: [f64; 3],
    lattice: [[f64; 3]; 3],
) -> [f64; 3] {
    nearest_periodic_image_fractional_with_axes(
        anchor_fractional,
        target_fractional,
        lattice,
        [true, true, true],
    )
}

pub fn nearest_periodic_image_fractional_with_axes(
    anchor_fractional: [f64; 3],
    target_fractional: [f64; 3],
    lattice: [[f64; 3]; 3],
    periodic_axes: [bool; 3],
) -> [f64; 3] {
    let base_delta = [
        if periodic_axes[0] {
            (target_fractional[0] - anchor_fractional[0]).rem_euclid(1.0)
        } else {
            target_fractional[0] - anchor_fractional[0]
        },
        if periodic_axes[1] {
            (target_fractional[1] - anchor_fractional[1]).rem_euclid(1.0)
        } else {
            target_fractional[1] - anchor_fractional[1]
        },
        if periodic_axes[2] {
            (target_fractional[2] - anchor_fractional[2]).rem_euclid(1.0)
        } else {
            target_fractional[2] - anchor_fractional[2]
        },
    ];

    let mut best = target_fractional;
    let mut best_dist_sq = f64::INFINITY;
    for i in if periodic_axes[0] { -2..=2 } else { 0..=0 } {
        for j in if periodic_axes[1] { -2..=2 } else { 0..=0 } {
            for k in if periodic_axes[2] { -2..=2 } else { 0..=0 } {
                let candidate = [
                    anchor_fractional[0] + base_delta[0] + i as f64,
                    anchor_fractional[1] + base_delta[1] + j as f64,
                    anchor_fractional[2] + base_delta[2] + k as f64,
                ];
                let delta = [
                    candidate[0] - anchor_fractional[0],
                    candidate[1] - anchor_fractional[1],
                    candidate[2] - anchor_fractional[2],
                ];
                let cart = fractional_to_cartesian(lattice, delta);
                let dist_sq = cart[0] * cart[0] + cart[1] * cart[1] + cart[2] * cart[2];
                if dist_sq < best_dist_sq {
                    best_dist_sq = dist_sq;
                    best = candidate;
                }
            }
        }
    }
    best
}

fn vector_norm(value: [f64; 3]) -> f64 {
    (value[0] * value[0] + value[1] * value[1] + value[2] * value[2]).sqrt()
}

fn angle_deg(left: [f64; 3], right: [f64; 3]) -> f64 {
    let left_norm = vector_norm(left);
    let right_norm = vector_norm(right);
    if left_norm <= 1.0e-12 || right_norm <= 1.0e-12 {
        return f64::NAN;
    }
    let cosine = ((left[0] * right[0] + left[1] * right[1] + left[2] * right[2])
        / (left_norm * right_norm))
        .clamp(-1.0, 1.0);
    cosine.acos().to_degrees()
}

fn snap_angle(angle: f64) -> f64 {
    for target in [60.0, 90.0, 120.0] {
        if (angle - target).abs() < 1.0e-10 {
            return target;
        }
    }
    angle
}

fn multiply_matrix_vector(matrix: [[f64; 3]; 3], vector: [f64; 3]) -> [f64; 3] {
    [
        matrix[0][0] * vector[0] + matrix[0][1] * vector[1] + matrix[0][2] * vector[2],
        matrix[1][0] * vector[0] + matrix[1][1] * vector[1] + matrix[1][2] * vector[2],
        matrix[2][0] * vector[0] + matrix[2][1] * vector[1] + matrix[2][2] * vector[2],
    ]
}

fn invert_basis_matrix(lattice: [[f64; 3]; 3]) -> Option<[[f64; 3]; 3]> {
    let matrix = [
        [lattice[0][0], lattice[1][0], lattice[2][0]],
        [lattice[0][1], lattice[1][1], lattice[2][1]],
        [lattice[0][2], lattice[1][2], lattice[2][2]],
    ];
    let determinant = matrix[0][0] * (matrix[1][1] * matrix[2][2] - matrix[1][2] * matrix[2][1])
        - matrix[0][1] * (matrix[1][0] * matrix[2][2] - matrix[1][2] * matrix[2][0])
        + matrix[0][2] * (matrix[1][0] * matrix[2][1] - matrix[1][1] * matrix[2][0]);
    if determinant.abs() < 1.0e-12 {
        return None;
    }
    let inverse_determinant = 1.0 / determinant;
    Some([
        [
            (matrix[1][1] * matrix[2][2] - matrix[1][2] * matrix[2][1]) * inverse_determinant,
            (matrix[0][2] * matrix[2][1] - matrix[0][1] * matrix[2][2]) * inverse_determinant,
            (matrix[0][1] * matrix[1][2] - matrix[0][2] * matrix[1][1]) * inverse_determinant,
        ],
        [
            (matrix[1][2] * matrix[2][0] - matrix[1][0] * matrix[2][2]) * inverse_determinant,
            (matrix[0][0] * matrix[2][2] - matrix[0][2] * matrix[2][0]) * inverse_determinant,
            (matrix[0][2] * matrix[1][0] - matrix[0][0] * matrix[1][2]) * inverse_determinant,
        ],
        [
            (matrix[1][0] * matrix[2][1] - matrix[1][1] * matrix[2][0]) * inverse_determinant,
            (matrix[0][1] * matrix[2][0] - matrix[0][0] * matrix[2][1]) * inverse_determinant,
            (matrix[0][0] * matrix[1][1] - matrix[0][1] * matrix[1][0]) * inverse_determinant,
        ],
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lattice_parameter_roundtrip_preserves_standard_cell() {
        let params = UnitCellParameters {
            a: 4.0,
            b: 5.0,
            c: 6.0,
            alpha_deg: 90.0,
            beta_deg: 90.0,
            gamma_deg: 120.0,
        };
        let lattice = lattice_vectors_from_params(params);
        let recovered = lattice_params_from_vectors(lattice);
        assert!((recovered.a - 4.0).abs() < 1.0e-10);
        assert!((recovered.b - 5.0).abs() < 1.0e-10);
        assert!((recovered.c - 6.0).abs() < 1.0e-10);
        assert!((recovered.alpha_deg - 90.0).abs() < 1.0e-10);
        assert!((recovered.beta_deg - 90.0).abs() < 1.0e-10);
        assert!((recovered.gamma_deg - 120.0).abs() < 1.0e-10);
    }

    #[test]
    fn minimum_image_cartesian_distance_handles_non_orthorhombic_cells() {
        let lattice = lattice_vectors_from_params(UnitCellParameters {
            a: 4.0,
            b: 4.0,
            c: 4.0,
            alpha_deg: 90.0,
            beta_deg: 90.0,
            gamma_deg: 120.0,
        });
        let left = fractional_to_cartesian(lattice, [0.95, 0.05, 0.0]);
        let right = fractional_to_cartesian(lattice, [0.05, 0.05, 0.0]);
        let dist_sq = minimum_image_cartesian_distance_sq(left, right, Some(lattice));
        assert!(dist_sq < 0.25);
    }

    #[test]
    fn axis_aware_normalization_only_wraps_enabled_axes() {
        let coord =
            normalize_fractional_coordinate_with_axes([-0.2, 1.2, 3.5], [true, true, false]);
        assert!((coord[0] - 0.8).abs() < 1.0e-12);
        assert!((coord[1] - 0.2).abs() < 1.0e-12);
        assert!((coord[2] - 3.5).abs() < 1.0e-12);
    }
}
