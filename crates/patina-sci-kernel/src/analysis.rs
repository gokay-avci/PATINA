use serde::{Deserialize, Serialize};

#[cfg(feature = "bridge-patina-types")]
use crate::geometry::{fractional_to_cartesian, nearest_periodic_image_fractional_with_axes};

#[cfg(feature = "bridge-patina-types")]
use patina_types::Candidate;

#[cfg(feature = "bridge-patina-types")]
use nalgebra::{Matrix3, SymmetricEigen};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrincipalMomentAnalysis {
    pub normalized_moments: [f64; 3],
    pub center_of_mass: [f64; 3],
    pub inertia_tensor: [[f64; 3]; 3],
}

pub fn atomic_mass(species: &str) -> f64 {
    match species.trim() {
        "H" => 1.00794,
        "He" => 4.002602,
        "Li" => 6.941,
        "Be" => 9.012182,
        "B" => 10.811,
        "C" => 12.0107,
        "N" => 14.0067,
        "O" => 15.9994,
        "F" => 18.9984032,
        "Ne" => 20.1797,
        "Na" => 22.98976928,
        "Mg" => 24.3050,
        "Al" => 26.9815386,
        "Si" => 28.0855,
        "P" => 30.973762,
        "S" => 32.065,
        "Cl" => 35.453,
        "Ar" => 39.948,
        "K" => 39.0983,
        "Ca" => 40.078,
        "Sc" => 44.955912,
        "Ti" => 47.867,
        "V" => 50.9415,
        "Cr" => 51.9961,
        "Mn" => 54.938045,
        "Fe" => 55.845,
        "Co" => 58.933195,
        "Ni" => 58.6934,
        "Cu" => 63.546,
        "Zn" => 65.38,
        "Ga" => 69.723,
        "Ge" => 72.64,
        "As" => 74.92160,
        "Se" => 78.96,
        "Br" => 79.904,
        "Kr" => 83.798,
        "Rb" => 85.4678,
        "Sr" => 87.62,
        "Y" => 88.90585,
        "Zr" => 91.224,
        "Nb" => 92.90638,
        "Mo" => 95.96,
        "Ru" => 101.07,
        "Rh" => 102.90550,
        "Pd" => 106.42,
        "Ag" => 107.8682,
        "Cd" => 112.411,
        "In" => 114.818,
        "Sn" => 118.710,
        "Sb" => 121.760,
        "Te" => 127.60,
        "I" => 126.90447,
        "Xe" => 131.293,
        "Cs" => 132.9054519,
        "Ba" => 137.327,
        "La" => 138.90547,
        "Ce" => 140.116,
        "Pr" => 140.90765,
        "Nd" => 144.242,
        "Sm" => 150.36,
        "Eu" => 151.964,
        "Gd" => 157.25,
        "Tb" => 158.92535,
        "Dy" => 162.500,
        "Ho" => 164.93032,
        "Er" => 167.259,
        "Tm" => 168.93421,
        "Yb" => 173.054,
        "Lu" => 174.9668,
        "Hf" => 178.49,
        "Ta" => 180.94788,
        "W" => 183.84,
        "Re" => 186.207,
        "Os" => 190.23,
        "Ir" => 192.217,
        "Pt" => 195.084,
        "Au" => 196.966569,
        "Hg" => 200.59,
        "Pb" => 207.2,
        _ => 1.0,
    }
}

pub fn center_of_mass_cartesian(species: &[String], cartesian_positions: &[[f64; 3]]) -> [f64; 3] {
    let mut center = [0.0; 3];
    let mut total_mass = 0.0;
    for (species, position) in species.iter().zip(cartesian_positions.iter()) {
        let mass = atomic_mass(species);
        total_mass += mass;
        center[0] += position[0] * mass;
        center[1] += position[1] * mass;
        center[2] += position[2] * mass;
    }
    let denom = total_mass.max(1.0e-12);
    [center[0] / denom, center[1] / denom, center[2] / denom]
}

pub fn inertia_tensor_cartesian(
    species: &[String],
    cartesian_positions: &[[f64; 3]],
) -> [[f64; 3]; 3] {
    let center = center_of_mass_cartesian(species, cartesian_positions);
    let mut tensor = [[0.0; 3]; 3];
    for (species, position) in species.iter().zip(cartesian_positions.iter()) {
        let mass = atomic_mass(species);
        let shifted = [
            position[0] - center[0],
            position[1] - center[1],
            position[2] - center[2],
        ];
        let r2 = shifted[0] * shifted[0] + shifted[1] * shifted[1] + shifted[2] * shifted[2];
        for row in 0..3 {
            for col in 0..3 {
                let identity = if row == col { 1.0 } else { 0.0 };
                tensor[row][col] += mass * (r2 * identity - shifted[row] * shifted[col]);
            }
        }
    }
    tensor
}

#[cfg(feature = "bridge-patina-types")]
pub fn candidate_cartesian_positions(candidate: &Candidate) -> Vec<[f64; 3]> {
    match candidate.lattice {
        Some(lattice) if !candidate.fractional_coords.is_empty() => {
            let anchor = candidate.fractional_coords[0];
            candidate
                .fractional_coords
                .iter()
                .map(|&fractional| {
                    let unwrapped = nearest_periodic_image_fractional_with_axes(
                        anchor,
                        fractional,
                        lattice,
                        candidate.periodic_axes,
                    );
                    fractional_to_cartesian(lattice, unwrapped)
                })
                .collect()
        }
        _ => candidate.fractional_coords.clone(),
    }
}

#[cfg(feature = "bridge-patina-types")]
pub fn candidate_center_of_mass(candidate: &Candidate) -> [f64; 3] {
    let positions = candidate_cartesian_positions(candidate);
    center_of_mass_cartesian(&candidate.species, &positions)
}

#[cfg(feature = "bridge-patina-types")]
pub fn candidate_inertia_tensor(candidate: &Candidate) -> [[f64; 3]; 3] {
    let positions = candidate_cartesian_positions(candidate);
    inertia_tensor_cartesian(&candidate.species, &positions)
}

#[cfg(feature = "bridge-patina-types")]
pub fn candidate_principal_moment_analysis(candidate: &Candidate) -> PrincipalMomentAnalysis {
    let positions = candidate_cartesian_positions(candidate);
    let center_of_mass = center_of_mass_cartesian(&candidate.species, &positions);
    let inertia_tensor = inertia_tensor_cartesian(&candidate.species, &positions);
    let matrix = Matrix3::new(
        inertia_tensor[0][0],
        inertia_tensor[0][1],
        inertia_tensor[0][2],
        inertia_tensor[1][0],
        inertia_tensor[1][1],
        inertia_tensor[1][2],
        inertia_tensor[2][0],
        inertia_tensor[2][1],
        inertia_tensor[2][2],
    );
    let eig = SymmetricEigen::new(matrix);
    let mut moments = [eig.eigenvalues[0], eig.eigenvalues[1], eig.eigenvalues[2]];
    moments.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let sum = moments.iter().copied().sum::<f64>().abs().max(1.0e-12);
    PrincipalMomentAnalysis {
        normalized_moments: [moments[0] / sum, moments[1] / sum, moments[2] / sum],
        center_of_mass,
        inertia_tensor,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "bridge-patina-types")]
    use patina_types::Candidate;

    #[test]
    fn atomic_mass_defaults_unknown_species_to_one() {
        assert!((atomic_mass("Unobtainium") - 1.0).abs() < 1.0e-12);
        assert!((atomic_mass("O") - 15.9994).abs() < 1.0e-12);
    }

    #[test]
    #[cfg(feature = "bridge-patina-types")]
    fn periodic_pmoi_is_stable_under_unit_cell_wrapping() {
        let lattice = [[8.0, 0.0, 0.0], [0.0, 9.0, 0.0], [0.0, 0.0, 10.0]];
        let canonical = Candidate {
            species: vec!["Mg".into(), "O".into(), "Mg".into()],
            fractional_coords: vec![[0.1, 0.2, 0.3], [0.18, 0.24, 0.32], [0.26, 0.21, 0.29]],
            lattice: Some(lattice),
            periodic_axes: [true, true, true],
            label: "canonical".into(),
        };
        let wrapped = Candidate {
            species: canonical.species.clone(),
            fractional_coords: vec![[1.1, 0.2, 0.3], [-0.82, 0.24, 0.32], [0.26, 1.21, -0.71]],
            lattice: Some(lattice),
            periodic_axes: [true, true, true],
            label: "wrapped".into(),
        };

        let left = candidate_principal_moment_analysis(&canonical).normalized_moments;
        let right = candidate_principal_moment_analysis(&wrapped).normalized_moments;
        for (left, right) in left.into_iter().zip(right) {
            assert!((left - right).abs() < 1.0e-9);
        }
    }
}
