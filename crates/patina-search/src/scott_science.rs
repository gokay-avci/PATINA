use nalgebra::Matrix3;
pub use patina_sci_kernel::{
    assess_periodic_cell, build_structure_edges, candidate_center_of_mass,
    candidate_inertia_tensor, candidate_principal_moment_analysis, cartesian_to_fractional,
    classify_topology_verdict, fractional_to_cartesian, lattice_params_from_vectors,
    lattice_vectors_from_params, minimum_image_cartesian_distance_sq,
    minimum_image_cartesian_distance_sq_with_axes, nearest_periodic_image_fractional,
    nearest_periodic_image_fractional_with_axes, normalize_fractional_coordinate,
    normalize_fractional_coordinate_with_axes, summarize_coordination_histograms,
    summarize_edge_pairs, AtomSpec, CellAssessment, GeometryRejectionReason, HashkeyRadiusMode,
    TopologyAtom, TopologyError, UnitCellParameters,
};
use patina_types::Candidate;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub struct GeometryAssessment {
    pub rejection: Option<GeometryRejectionReason>,
    pub minimum_pair_distance: Option<f64>,
    pub zero_coordination_atoms: usize,
    pub connected_components: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScottScienceError {
    Topology(TopologyError),
}

impl std::fmt::Display for ScottScienceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Topology(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for ScottScienceError {}

pub fn scott_intent_pmoi(candidate: &Candidate) -> [f64; 3] {
    normalized_pmoi(candidate)
}

pub fn scott_intent_pmoi_difference(left: &Candidate, right: &Candidate) -> f64 {
    let left_pmoi = normalized_pmoi(left);
    let right_pmoi = normalized_pmoi(right);
    left_pmoi
        .iter()
        .zip(right_pmoi.iter())
        .map(|(l, r)| (l - r).abs())
        .sum::<f64>()
}

pub fn compare_pmoi_scott_intent(left: &Candidate, right: &Candidate, tolerance: f64) -> bool {
    if !left.is_zero_d() || !right.is_zero_d() {
        return false;
    }
    scott_intent_pmoi_difference(left, right) <= tolerance
}

pub fn compute_structure_hashkey_radius(
    species_counts: &BTreeMap<String, usize>,
    atom_specs: &[AtomSpec],
    radius_mode: HashkeyRadiusMode,
    radius_const: f64,
) -> Result<f64, ScottScienceError> {
    patina_sci_kernel::compute_structure_hashkey_radius(
        species_counts,
        atom_specs,
        radius_mode,
        radius_const,
    )
    .map_err(ScottScienceError::Topology)
}

pub fn assess_cluster_geometry(
    species: &[String],
    coords: &[[f64; 3]],
    collapse_radii: &BTreeMap<(String, String), f64>,
    default_collapse_radius: f64,
    fragment_radii: &BTreeMap<(String, String), f64>,
    default_fragment_radius: f64,
) -> GeometryAssessment {
    if coords.len() < 2 {
        return GeometryAssessment {
            rejection: None,
            minimum_pair_distance: None,
            zero_coordination_atoms: 0,
            connected_components: usize::from(!coords.is_empty()),
        };
    }

    let mut minimum_pair_distance = f64::INFINITY;
    let mut adjacency = vec![Vec::<usize>::new(); coords.len()];

    for i in 0..coords.len() {
        for j in (i + 1)..coords.len() {
            let distance = pair_distance(coords[i], coords[j]);
            minimum_pair_distance = minimum_pair_distance.min(distance);

            let collapse_radius = collapse_radii
                .get(&species_pair_key(&species[i], &species[j]))
                .copied()
                .unwrap_or(default_collapse_radius);
            if distance < collapse_radius {
                return GeometryAssessment {
                    rejection: Some(GeometryRejectionReason::Collapsed),
                    minimum_pair_distance: Some(distance),
                    zero_coordination_atoms: 0,
                    connected_components: coords.len(),
                };
            }

            let fragment_radius = fragment_radii
                .get(&species_pair_key(&species[i], &species[j]))
                .copied()
                .unwrap_or(default_fragment_radius);
            if distance <= fragment_radius {
                adjacency[i].push(j);
                adjacency[j].push(i);
            }
        }
    }

    let zero_coordination_atoms = adjacency
        .iter()
        .filter(|neighbors| neighbors.is_empty())
        .count();
    let connected_components = connected_components(&adjacency);
    let rejection = if connected_components > 1 {
        Some(GeometryRejectionReason::Fragmented)
    } else if zero_coordination_atoms > 0 {
        Some(GeometryRejectionReason::ZeroCoordinated)
    } else {
        None
    };

    GeometryAssessment {
        rejection,
        minimum_pair_distance: minimum_pair_distance
            .is_finite()
            .then_some(minimum_pair_distance),
        zero_coordination_atoms,
        connected_components,
    }
}

pub fn normalize_periodic_candidate(candidate: &mut Candidate) {
    if candidate.lattice.is_none() {
        return;
    }
    let periodic_axes = candidate.periodic_axes;
    for coord in &mut candidate.fractional_coords {
        *coord = normalize_fractional_coordinate_with_axes(*coord, periodic_axes);
    }
}

pub fn periodic_neighbors_within_cutoff(
    fractional_coords: &[[f64; 3]],
    lattice: [[f64; 3]; 3],
    cutoff: f64,
) -> Vec<Vec<(usize, f64)>> {
    periodic_neighbors_within_cutoff_with_axes(
        fractional_coords,
        lattice,
        [true, true, true],
        cutoff,
    )
}

pub fn periodic_neighbors_within_cutoff_with_axes(
    fractional_coords: &[[f64; 3]],
    lattice: [[f64; 3]; 3],
    periodic_axes: [bool; 3],
    cutoff: f64,
) -> Vec<Vec<(usize, f64)>> {
    let cutoff_sq = cutoff * cutoff;
    let mut neighbors = vec![Vec::new(); fractional_coords.len()];
    for i in 0..fractional_coords.len() {
        for j in 0..fractional_coords.len() {
            if i == j {
                continue;
            }
            let wrapped = nearest_periodic_image_fractional_with_axes(
                fractional_coords[i],
                fractional_coords[j],
                lattice,
                periodic_axes,
            );
            let left_cart = fractional_to_cartesian(lattice, fractional_coords[i]);
            let right_cart = fractional_to_cartesian(lattice, wrapped);
            let dist_sq = minimum_image_cartesian_distance_sq(left_cart, right_cart, None);
            if dist_sq <= cutoff_sq {
                neighbors[i].push((j, dist_sq.sqrt()));
            }
        }
    }
    neighbors
}

pub fn assess_candidate_geometry(
    candidate: &Candidate,
    collapse_radii: &BTreeMap<(String, String), f64>,
    default_collapse_radius: f64,
    fragment_radii: &BTreeMap<(String, String), f64>,
    default_fragment_radius: f64,
) -> GeometryAssessment {
    if let Some(lattice) = candidate.lattice {
        let cell = assess_periodic_cell(lattice);
        if let Some(rejection) = cell.rejection {
            return GeometryAssessment {
                rejection: Some(rejection),
                minimum_pair_distance: None,
                zero_coordination_atoms: 0,
                connected_components: candidate.len(),
            };
        }
        assess_periodic_geometry(
            &candidate.species,
            &candidate.fractional_coords,
            lattice,
            candidate.periodic_axes,
            collapse_radii,
            default_collapse_radius,
        )
    } else {
        assess_cluster_geometry(
            &candidate.species,
            &candidate.fractional_coords,
            collapse_radii,
            default_collapse_radius,
            fragment_radii,
            default_fragment_radius,
        )
    }
}

fn assess_periodic_geometry(
    species: &[String],
    coords: &[[f64; 3]],
    lattice: [[f64; 3]; 3],
    periodic_axes: [bool; 3],
    collapse_radii: &BTreeMap<(String, String), f64>,
    default_collapse_radius: f64,
) -> GeometryAssessment {
    if coords.len() < 2 {
        return GeometryAssessment {
            rejection: None,
            minimum_pair_distance: None,
            zero_coordination_atoms: 0,
            connected_components: usize::from(!coords.is_empty()),
        };
    }

    let mut minimum_pair_distance = f64::INFINITY;
    for i in 0..coords.len() {
        for j in (i + 1)..coords.len() {
            let distance = periodic_pair_distance(coords[i], coords[j], lattice, periodic_axes);
            minimum_pair_distance = minimum_pair_distance.min(distance);
            let collapse_radius = collapse_radii
                .get(&species_pair_key(&species[i], &species[j]))
                .copied()
                .unwrap_or(default_collapse_radius);
            if distance < collapse_radius {
                return GeometryAssessment {
                    rejection: Some(GeometryRejectionReason::Collapsed),
                    minimum_pair_distance: Some(distance),
                    zero_coordination_atoms: 0,
                    connected_components: 1,
                };
            }
        }
    }

    GeometryAssessment {
        rejection: None,
        minimum_pair_distance: minimum_pair_distance
            .is_finite()
            .then_some(minimum_pair_distance),
        zero_coordination_atoms: 0,
        connected_components: 1,
    }
}

fn normalized_pmoi(candidate: &Candidate) -> [f64; 3] {
    candidate_principal_moment_analysis(candidate).normalized_moments
}

pub(crate) fn inertia_tensor(candidate: &Candidate) -> Matrix3<f64> {
    let tensor = candidate_inertia_tensor(candidate);
    Matrix3::new(
        tensor[0][0],
        tensor[0][1],
        tensor[0][2],
        tensor[1][0],
        tensor[1][1],
        tensor[1][2],
        tensor[2][0],
        tensor[2][1],
        tensor[2][2],
    )
}

pub(crate) fn center_of_mass(candidate: &Candidate) -> [f64; 3] {
    candidate_center_of_mass(candidate)
}

fn pair_distance(left: [f64; 3], right: [f64; 3]) -> f64 {
    let dx = left[0] - right[0];
    let dy = left[1] - right[1];
    let dz = left[2] - right[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

fn periodic_pair_distance(
    left: [f64; 3],
    right: [f64; 3],
    lattice: [[f64; 3]; 3],
    periodic_axes: [bool; 3],
) -> f64 {
    let mut delta = [left[0] - right[0], left[1] - right[1], left[2] - right[2]];
    // A lattice-backed candidate is not always fully periodic; slabs and wires must only wrap
    // along their declared periodic axes.
    for axis in 0..3 {
        if periodic_axes[axis] {
            delta[axis] -= delta[axis].round();
        }
    }
    let cart = [
        delta[0] * lattice[0][0] + delta[1] * lattice[1][0] + delta[2] * lattice[2][0],
        delta[0] * lattice[0][1] + delta[1] * lattice[1][1] + delta[2] * lattice[2][1],
        delta[0] * lattice[0][2] + delta[1] * lattice[1][2] + delta[2] * lattice[2][2],
    ];
    vector_norm(cart)
}

fn species_pair_key(left: &str, right: &str) -> (String, String) {
    if left <= right {
        (left.to_string(), right.to_string())
    } else {
        (right.to_string(), left.to_string())
    }
}

fn vector_norm(vector: [f64; 3]) -> f64 {
    (vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2]).sqrt()
}

fn connected_components(adjacency: &[Vec<usize>]) -> usize {
    if adjacency.is_empty() {
        return 0;
    }

    let mut visited = vec![false; adjacency.len()];
    let mut components = 0usize;
    for start in 0..adjacency.len() {
        if visited[start] {
            continue;
        }
        components += 1;
        let mut stack = vec![start];
        visited[start] = true;
        while let Some(node) = stack.pop() {
            for &next in &adjacency[node] {
                if !visited[next] {
                    visited[next] = true;
                    stack.push(next);
                }
            }
        }
    }
    components
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashkey_radius_uses_requested_mode() {
        let species_counts =
            BTreeMap::from([(String::from("Mg"), 1usize), (String::from("O"), 1usize)]);
        let atom_specs = vec![
            AtomSpec {
                species: "Mg".into(),
                covalent_radius: 1.41,
                ionic_radius: 0.86,
            },
            AtomSpec {
                species: "O".into(),
                covalent_radius: 0.66,
                ionic_radius: 1.26,
            },
        ];

        let ionic = compute_structure_hashkey_radius(
            &species_counts,
            &atom_specs,
            HashkeyRadiusMode::Ionic,
            0.2,
        )
        .unwrap();
        let covalent = compute_structure_hashkey_radius(
            &species_counts,
            &atom_specs,
            HashkeyRadiusMode::Covalent,
            0.2,
        )
        .unwrap();

        assert!((ionic - (0.86 + 1.26 + 0.2)).abs() < 1.0e-12);
        assert!((covalent - (1.41 + 0.66 + 0.2)).abs() < 1.0e-12);
    }

    #[test]
    fn cluster_geometry_rejects_collapsed_pairs_before_other_checks() {
        let assessment = assess_cluster_geometry(
            &["Mg".into(), "O".into()],
            &[[0.0, 0.0, 0.0], [0.2, 0.0, 0.0]],
            &BTreeMap::from([((String::from("Mg"), String::from("O")), 0.5)]),
            0.5,
            &BTreeMap::from([((String::from("Mg"), String::from("O")), 1.5)]),
            1.5,
        );

        assert_eq!(
            assessment.rejection,
            Some(GeometryRejectionReason::Collapsed)
        );
    }

    #[test]
    fn cluster_geometry_flags_fragmentation_before_zero_coordination() {
        let assessment = assess_cluster_geometry(
            &["Mg".into(), "O".into(), "Mg".into()],
            &[[0.0, 0.0, 0.0], [0.9, 0.0, 0.0], [5.0, 0.0, 0.0]],
            &BTreeMap::from([
                ((String::from("Mg"), String::from("O")), 0.4),
                ((String::from("Mg"), String::from("Mg")), 0.4),
            ]),
            0.4,
            &BTreeMap::from([
                ((String::from("Mg"), String::from("O")), 1.2),
                ((String::from("Mg"), String::from("Mg")), 1.2),
            ]),
            1.2,
        );

        assert_eq!(
            assessment.rejection,
            Some(GeometryRejectionReason::Fragmented)
        );
        assert_eq!(assessment.zero_coordination_atoms, 1);
        assert!(assessment.connected_components > 1);
    }

    #[test]
    fn periodic_cell_rejects_unphysical_angles() {
        let cell = assess_periodic_cell([[4.0, 0.0, 0.0], [3.9, 0.1, 0.0], [3.9, -0.1, 0.0]]);

        assert_eq!(
            cell.rejection,
            Some(GeometryRejectionReason::UnphysicalCell)
        );
    }

    #[test]
    fn periodic_geometry_uses_minimum_image_collapse_check() {
        let candidate = Candidate {
            species: vec!["Mg".into(), "O".into()],
            fractional_coords: vec![[0.1, 0.0, 0.0], [0.9, 0.0, 0.0]],
            lattice: Some([[2.0, 0.0, 0.0], [0.0, 6.0, 0.0], [0.0, 0.0, 6.0]]),
            periodic_axes: [true, true, true],
            label: "periodic".into(),
        };

        let assessment = assess_candidate_geometry(
            &candidate,
            &BTreeMap::from([((String::from("Mg"), String::from("O")), 0.5)]),
            0.5,
            &BTreeMap::new(),
            1.2,
        );

        assert_eq!(
            assessment.rejection,
            Some(GeometryRejectionReason::Collapsed)
        );
        assert!(assessment.minimum_pair_distance.unwrap() < 0.5);
    }

    #[test]
    fn lattice_parameter_roundtrip_preserves_standard_cell() {
        let params = UnitCellParameters {
            a: 4.5,
            b: 5.5,
            c: 6.5,
            alpha_deg: 90.0,
            beta_deg: 100.0,
            gamma_deg: 120.0,
        };

        let lattice = lattice_vectors_from_params(params);
        let recovered = lattice_params_from_vectors(lattice);

        assert!((recovered.a - params.a).abs() < 1.0e-9);
        assert!((recovered.b - params.b).abs() < 1.0e-9);
        assert!((recovered.c - params.c).abs() < 1.0e-9);
        assert!((recovered.alpha_deg - params.alpha_deg).abs() < 1.0e-9);
        assert!((recovered.beta_deg - params.beta_deg).abs() < 1.0e-9);
        assert!((recovered.gamma_deg - params.gamma_deg).abs() < 1.0e-9);
    }

    #[test]
    fn fractional_cartesian_roundtrip_is_stable() {
        let lattice = lattice_vectors_from_params(UnitCellParameters {
            a: 5.0,
            b: 6.0,
            c: 7.0,
            alpha_deg: 90.0,
            beta_deg: 95.0,
            gamma_deg: 105.0,
        });
        let fractional = [0.2, 0.35, 0.8];
        let cart = fractional_to_cartesian(lattice, fractional);
        let recovered = cartesian_to_fractional(lattice, cart).unwrap();

        assert!((recovered[0] - fractional[0]).abs() < 1.0e-9);
        assert!((recovered[1] - fractional[1]).abs() < 1.0e-9);
        assert!((recovered[2] - fractional[2]).abs() < 1.0e-9);
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
    fn periodic_pmoi_uses_cartesian_positions_instead_of_fractional_axes() {
        let lattice = [[10.0, 0.0, 0.0], [0.0, 20.0, 0.0], [0.0, 0.0, 30.0]];
        let candidate = Candidate {
            species: vec!["Mg".into(), "O".into(), "O".into()],
            fractional_coords: vec![[0.0, 0.0, 0.0], [0.2, 0.0, 0.0], [0.0, 0.1, 0.0]],
            lattice: Some(lattice),
            periodic_axes: [true, true, true],
            label: "periodic_pmoi".into(),
        };

        let pmoi = normalized_pmoi(&candidate);
        let cartesian_equivalent = Candidate {
            species: candidate.species.clone(),
            fractional_coords: vec![[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [0.0, 2.0, 0.0]],
            lattice: None,
            periodic_axes: [false, false, false],
            label: "cartesian_pmoi".into(),
        };
        let cartesian_pmoi = normalized_pmoi(&cartesian_equivalent);

        for (left, right) in pmoi.into_iter().zip(cartesian_pmoi) {
            assert!((left - right).abs() < 1.0e-9);
        }
    }

    #[test]
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

        let canonical_pmoi = normalized_pmoi(&canonical);
        let wrapped_pmoi = normalized_pmoi(&wrapped);
        for (left, right) in canonical_pmoi.into_iter().zip(wrapped_pmoi) {
            assert!((left - right).abs() < 1.0e-9);
        }
    }

    #[test]
    fn normalize_periodic_candidate_wraps_fractional_coords_into_unit_cell() {
        let mut candidate = Candidate {
            species: vec!["Mg".into(), "O".into()],
            fractional_coords: vec![[-0.2, 1.2, 0.5], [1.7, -0.1, 2.0]],
            lattice: Some([[4.0, 0.0, 0.0], [0.0, 4.0, 0.0], [0.0, 0.0, 4.0]]),
            periodic_axes: [true, true, true],
            label: "wrapped".into(),
        };

        normalize_periodic_candidate(&mut candidate);

        let first = candidate.fractional_coords[0];
        let second = candidate.fractional_coords[1];
        assert!((first[0] - 0.8).abs() < 1.0e-12);
        assert!((first[1] - 0.2).abs() < 1.0e-12);
        assert!((first[2] - 0.5).abs() < 1.0e-12);
        assert!((second[0] - 0.7).abs() < 1.0e-12);
        assert!((second[1] - 0.9).abs() < 1.0e-12);
        assert!(second[2].abs() < 1.0e-12);
    }

    #[test]
    fn nearest_periodic_image_matches_native_translate_intent() {
        let lattice = lattice_vectors_from_params(UnitCellParameters {
            a: 4.0,
            b: 4.0,
            c: 4.0,
            alpha_deg: 90.0,
            beta_deg: 90.0,
            gamma_deg: 120.0,
        });
        let nearest =
            nearest_periodic_image_fractional([0.95, 0.05, 0.0], [0.05, 0.05, 0.0], lattice);

        assert!((nearest[0] - 1.05).abs() < 1.0e-9);
        assert!((nearest[1] - 0.05).abs() < 1.0e-9);
    }

    #[test]
    fn periodic_neighbor_collection_uses_minimum_image_distance() {
        let lattice = lattice_vectors_from_params(UnitCellParameters {
            a: 2.0,
            b: 6.0,
            c: 6.0,
            alpha_deg: 90.0,
            beta_deg: 90.0,
            gamma_deg: 90.0,
        });
        let neighbors = periodic_neighbors_within_cutoff(
            &[[0.1, 0.0, 0.0], [0.9, 0.0, 0.0], [0.5, 0.5, 0.5]],
            lattice,
            0.5,
        );

        assert_eq!(neighbors[0].len(), 1);
        assert_eq!(neighbors[0][0].0, 1);
        assert!(neighbors[0][0].1 < 0.5);
        assert_eq!(neighbors[2].len(), 0);
    }

    #[test]
    fn axis_aware_minimum_image_only_wraps_enabled_axes() {
        let lattice = lattice_vectors_from_params(UnitCellParameters {
            a: 2.0,
            b: 2.0,
            c: 10.0,
            alpha_deg: 90.0,
            beta_deg: 90.0,
            gamma_deg: 90.0,
        });
        let left = fractional_to_cartesian(lattice, [0.1, 0.1, 0.1]);
        let right = fractional_to_cartesian(lattice, [0.9, 0.9, 0.1]);

        let x_only = minimum_image_cartesian_distance_sq_with_axes(
            left,
            right,
            Some(lattice),
            [true, false, false],
        );
        let xy = minimum_image_cartesian_distance_sq_with_axes(
            left,
            right,
            Some(lattice),
            [true, true, false],
        );

        assert!(xy < x_only);
        assert!(x_only > 1.0);
    }

    #[test]
    fn normalize_periodic_candidate_only_wraps_enabled_axes() {
        let mut candidate = Candidate {
            species: vec!["Mg".into(), "O".into()],
            fractional_coords: vec![[1.2, -0.3, 2.5], [-0.1, 1.4, -1.5]],
            lattice: Some([[4.0, 0.0, 0.0], [0.0, 6.0, 0.0], [0.0, 0.0, 20.0]]),
            periodic_axes: [true, true, false],
            label: "surface".into(),
        };

        normalize_periodic_candidate(&mut candidate);

        assert!((candidate.fractional_coords[0][0] - 0.2).abs() < 1.0e-12);
        assert!((candidate.fractional_coords[0][1] - 0.7).abs() < 1.0e-12);
        assert!((candidate.fractional_coords[0][2] - 2.5).abs() < 1.0e-12);
        assert!((candidate.fractional_coords[1][0] - 0.9).abs() < 1.0e-12);
        assert!((candidate.fractional_coords[1][1] - 0.4).abs() < 1.0e-12);
        assert!((candidate.fractional_coords[1][2] + 1.5).abs() < 1.0e-12);
    }

    #[test]
    fn partial_periodic_geometry_does_not_wrap_nonperiodic_axes_for_collapse_checks() {
        let candidate = Candidate {
            species: vec!["Mg".into(), "O".into()],
            fractional_coords: vec![[0.1, 0.1, 0.1], [0.1, 0.1, 0.95]],
            lattice: Some([[4.0, 0.0, 0.0], [0.0, 6.0, 0.0], [0.0, 0.0, 10.0]]),
            periodic_axes: [true, true, false],
            label: "surface".into(),
        };
        let collapse_radii = BTreeMap::from([((String::from("Mg"), String::from("O")), 2.0)]);

        let assessment =
            assess_candidate_geometry(&candidate, &collapse_radii, 2.0, &BTreeMap::new(), 2.0);

        assert_eq!(assessment.rejection, None);
        assert_eq!(assessment.connected_components, 1);
        assert_eq!(assessment.zero_coordination_atoms, 0);
        assert_eq!(assessment.minimum_pair_distance, Some(8.5));
    }
}
