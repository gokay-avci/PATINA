use super::scott_science;
use super::{
    assess_candidate_geometry, normalize_fractional_coordinate_with_axes, GeometryAssessment,
    ScottGaOperatorConfig,
};
use nalgebra::{Matrix3, SymmetricEigen, Vector3};
use patina_types::Candidate;
use std::cmp::Ordering;
use std::collections::BTreeMap;

/// Candidate mutation and geometry helpers kept separate from controller orchestration.
///
/// This module owns the structure-shaping mechanics used by the Rust search controllers:
/// small RNG utilities, SCOTT-shaped geometry guards, and cluster alignment helpers.
/// Keeping them here lets `lib.rs` stay focused on workflow/controller surfaces.
#[derive(Debug, Clone)]
pub(crate) struct ScottGeometryModel {
    pub(crate) default_min_distance: f64,
    pub(crate) default_fragment_distance: f64,
    pub(crate) pair_min_distances: BTreeMap<(String, String), f64>,
    pub(crate) pair_fragment_distances: BTreeMap<(String, String), f64>,
}

impl ScottGeometryModel {
    pub(crate) fn from_base(base: &Candidate) -> Self {
        let mut pair_min_distances = BTreeMap::new();
        let mut pair_fragment_distances = BTreeMap::new();
        let mut min_sq = f64::INFINITY;
        for i in 0..base.len() {
            for j in (i + 1)..base.len() {
                let dist_sq = distance_sq(base.fractional_coords[i], base.fractional_coords[j]);
                min_sq = min_sq.min(dist_sq);
                let key = species_pair_key(&base.species[i], &base.species[j]);
                let distance = dist_sq.sqrt();
                pair_min_distances
                    .entry(key)
                    .and_modify(|value: &mut f64| *value = (*value).min(distance))
                    .or_insert(distance);
                pair_fragment_distances
                    .entry(species_pair_key(&base.species[i], &base.species[j]))
                    .and_modify(|value: &mut f64| *value = (*value).max(distance * 1.35))
                    .or_insert(distance * 1.35);
            }
        }

        let default_min_distance = if min_sq.is_finite() {
            min_sq.sqrt() * 0.6
        } else {
            0.8
        };
        Self {
            default_min_distance,
            default_fragment_distance: if min_sq.is_finite() {
                min_sq.sqrt() * 1.35
            } else {
                default_min_distance * 2.0
            },
            pair_min_distances,
            pair_fragment_distances,
        }
    }

    pub(crate) fn pair_floor(&self, left: &str, right: &str) -> f64 {
        self.pair_min_distances
            .get(&species_pair_key(left, right))
            .copied()
            .unwrap_or(self.default_min_distance)
    }

    pub(crate) fn pair_fragment_cutoff(&self, left: &str, right: &str) -> f64 {
        self.pair_fragment_distances
            .get(&species_pair_key(left, right))
            .copied()
            .unwrap_or(self.default_fragment_distance)
    }
}

fn species_pair_key(left: &str, right: &str) -> (String, String) {
    if left <= right {
        (left.to_string(), right.to_string())
    } else {
        (right.to_string(), left.to_string())
    }
}

pub(crate) fn assert_native_supported_search_periodicity(candidate: &Candidate, context: &str) {
    assert!(
        candidate.supports_native_scott_search(),
        "Scott parity search operators support only 0D clusters or full 3D periodic cells; {context} received periodic_axes={:?}",
        candidate.periodic_axes
    );
}

pub(crate) fn periodic_axis_indices(candidate: &Candidate) -> Vec<usize> {
    candidate
        .periodic_axes
        .iter()
        .enumerate()
        .filter_map(|(axis, enabled)| enabled.then_some(axis))
        .collect()
}

fn wrap_fractional_coordinate_axes(coord: &mut [f64; 3], periodic_axes: [bool; 3]) {
    *coord = normalize_fractional_coordinate_with_axes(*coord, periodic_axes);
}

pub(crate) fn perturb_candidate(candidate: &mut Candidate, step_size: f64, rng: &mut TinyRng) {
    assert_native_supported_search_periodicity(candidate, "perturb_candidate");
    let scale = step_size.abs();
    let periodic_axes = candidate.periodic_axes;
    for coord in &mut candidate.fractional_coords {
        for (axis, value) in coord.iter_mut().enumerate() {
            // Native SCOTT moveAtoms uses 2*R_BH_STEPSIZE*(r-0.5), i.e. a full
            // independent symmetric displacement in [-step_size, +step_size]
            // on each Cartesian axis for 0D threshold / quench walks.
            let delta = ((rng.next_f64() * 2.0) - 1.0) * scale;
            let updated = *value + delta;
            *value = updated;
            if periodic_axes[axis] {
                *value = value.rem_euclid(1.0);
            }
        }
    }
}

pub(crate) fn mutate_candidate(
    candidate: &mut Candidate,
    moves: usize,
    step_size: f64,
    rng: &mut TinyRng,
    operator_cfg: ScottGaOperatorConfig,
) {
    assert_native_supported_search_periodicity(candidate, "mutate_candidate");
    let move_count = moves.max(1).min(candidate.len().max(1));
    let scale = step_size.abs().max(0.05);
    let periodic_axes = candidate.periodic_axes;
    let mutation_type = rng.next_f64();

    if mutation_type < operator_cfg.mutate_expand_ratio {
        rescale_candidate(candidate, 1.0 + 0.18 * scale.max(0.25));
        return;
    }
    if mutation_type < operator_cfg.mutate_expand_ratio + operator_cfg.mutate_contract_ratio {
        rescale_candidate(candidate, (1.0 - 0.12 * scale.max(0.25)).max(0.7));
        return;
    }

    for _ in 0..move_count {
        let atom_idx = rng.next_usize(candidate.len().max(1));
        if mutation_type
            < operator_cfg.mutate_expand_ratio
                + operator_cfg.mutate_contract_ratio
                + operator_cfg.mutate_swap_ratio
            && candidate.len() > 1
        {
            let mut other = rng.next_usize(candidate.len());
            while other == atom_idx && candidate.len() > 1 {
                other = rng.next_usize(candidate.len());
            }
            candidate.fractional_coords.swap(atom_idx, other);
            continue;
        }

        for (axis, value) in candidate.fractional_coords[atom_idx].iter_mut().enumerate() {
            let delta = (rng.next_f64() - 0.5) * scale * 2.0;
            let updated = *value + delta;
            *value = updated;
            if periodic_axes[axis] {
                *value = value.rem_euclid(1.0);
            }
        }
    }
}

pub(crate) fn random_foreign_structure(
    base: &Candidate,
    geometry_model: &ScottGeometryModel,
    rng: &mut TinyRng,
) -> Candidate {
    assert_native_supported_search_periodicity(base, "random_foreign_structure");
    let mut candidate = base.clone();
    let periodic_axes = candidate.periodic_axes;
    if !candidate.is_zero_d() {
        for coord in &mut candidate.fractional_coords {
            for (axis, value) in coord.iter_mut().enumerate() {
                if periodic_axes[axis] {
                    *value = rng.next_f64();
                }
            }
            wrap_fractional_coordinate_axes(coord, periodic_axes);
        }
        return candidate;
    }

    let (mins, maxs) = bounding_box(&base.fractional_coords);
    let spans = [
        (maxs[0] - mins[0]).abs().max(2.0),
        (maxs[1] - mins[1]).abs().max(2.0),
        (maxs[2] - mins[2]).abs().max(2.0),
    ];
    let center = [
        0.5 * (mins[0] + maxs[0]),
        0.5 * (mins[1] + maxs[1]),
        0.5 * (mins[2] + maxs[2]),
    ];
    let mut placed = Vec::with_capacity(candidate.len());
    for atom_idx in 0..candidate.len() {
        let species = candidate.species[atom_idx].clone();
        let mut accepted = None;
        for _ in 0..128 {
            let proposal = [
                center[0] + (rng.next_f64() - 0.5) * spans[0] * 1.35,
                center[1] + (rng.next_f64() - 0.5) * spans[1] * 1.35,
                center[2] + (rng.next_f64() - 0.5) * spans[2] * 1.35,
            ];
            let ok = placed
                .iter()
                .all(|(other_species, other_coords): &(String, [f64; 3])| {
                    let floor = geometry_model.pair_floor(&species, other_species);
                    distance_sq(proposal, *other_coords) >= floor * floor
                });
            if ok {
                accepted = Some(proposal);
                break;
            }
        }

        let coord = accepted.unwrap_or([
            center[0] + (rng.next_f64() - 0.5) * spans[0] * 1.35,
            center[1] + (rng.next_f64() - 0.5) * spans[1] * 1.35,
            center[2] + (rng.next_f64() - 0.5) * spans[2] * 1.35,
        ]);
        candidate.fractional_coords[atom_idx] = coord;
        placed.push((species, coord));
    }
    standardize_candidate_coordinates(&mut candidate);
    candidate
}

pub(crate) fn geometry_is_reasonable(
    candidate: &Candidate,
    geometry_model: &ScottGeometryModel,
) -> bool {
    geometry_assessment(candidate, geometry_model)
        .rejection
        .is_none()
}

fn geometry_assessment(
    candidate: &Candidate,
    geometry_model: &ScottGeometryModel,
) -> GeometryAssessment {
    let mut collapse_radii = BTreeMap::new();
    let mut fragment_radii = BTreeMap::new();
    for key in geometry_model.pair_min_distances.keys() {
        collapse_radii.insert(key.clone(), geometry_model.pair_floor(&key.0, &key.1));
    }
    for key in geometry_model.pair_fragment_distances.keys() {
        fragment_radii.insert(
            key.clone(),
            geometry_model.pair_fragment_cutoff(&key.0, &key.1),
        );
    }

    assess_candidate_geometry(
        candidate,
        &collapse_radii,
        geometry_model.default_min_distance,
        &fragment_radii,
        geometry_model.default_fragment_distance,
    )
}

pub(crate) fn candidate_geometry_score(
    candidate: &Candidate,
    geometry_model: &ScottGeometryModel,
) -> f64 {
    let mut min_sq = f64::INFINITY;
    let mut collisions = 0usize;
    for i in 0..candidate.len() {
        for j in (i + 1)..candidate.len() {
            let dist_sq = distance_sq(
                candidate.fractional_coords[i],
                candidate.fractional_coords[j],
            );
            let floor = geometry_model.pair_floor(&candidate.species[i], &candidate.species[j]);
            if dist_sq < floor * floor {
                collisions += 1;
            }
            min_sq = min_sq.min(dist_sq);
        }
    }

    let center = center_of_geometry(&candidate.fractional_coords);
    let radial_spread = candidate
        .fractional_coords
        .iter()
        .map(|coord| {
            let dx = coord[0] - center[0];
            let dy = coord[1] - center[1];
            let dz = coord[2] - center[2];
            (dx * dx + dy * dy + dz * dz).sqrt()
        })
        .sum::<f64>();

    let min_dist = if min_sq.is_finite() {
        min_sq.sqrt()
    } else {
        0.0
    };
    min_dist + 0.05 * radial_spread - 10.0 * collisions as f64
}

fn bounding_box(coords: &[[f64; 3]]) -> ([f64; 3], [f64; 3]) {
    let mut mins = [f64::INFINITY; 3];
    let mut maxs = [f64::NEG_INFINITY; 3];
    for row in coords {
        for axis in 0..3 {
            mins[axis] = mins[axis].min(row[axis]);
            maxs[axis] = maxs[axis].max(row[axis]);
        }
    }
    (mins, maxs)
}

fn center_of_geometry(coords: &[[f64; 3]]) -> [f64; 3] {
    let n = coords.len().max(1) as f64;
    let mut center = [0.0; 3];
    for row in coords {
        center[0] += row[0];
        center[1] += row[1];
        center[2] += row[2];
    }
    center[0] /= n;
    center[1] /= n;
    center[2] /= n;
    center
}

pub(crate) fn cluster_intrinsic_dims(candidate: &Candidate, dim_tolerance: f64) -> usize {
    let (mins, maxs) = bounding_box(&candidate.fractional_coords);
    (0..3)
        .filter(|&axis| (maxs[axis] - mins[axis]).abs() > dim_tolerance)
        .count()
}

pub(crate) fn move_coords_to_com(coords: &mut [[f64; 3]]) {
    let center = center_of_geometry(coords);
    for coord in coords {
        coord[0] -= center[0];
        coord[1] -= center[1];
        coord[2] -= center[2];
    }
}

fn rescale_candidate(candidate: &mut Candidate, factor: f64) {
    let center = center_of_geometry(&candidate.fractional_coords);
    let periodic_axes = candidate.periodic_axes;
    for coord in &mut candidate.fractional_coords {
        for axis in 0..3 {
            let updated = center[axis] + (coord[axis] - center[axis]) * factor;
            coord[axis] = updated;
            if periodic_axes[axis] {
                coord[axis] = coord[axis].rem_euclid(1.0);
            }
        }
    }
}

pub(crate) fn standardize_candidate_coordinates(candidate: &mut Candidate) {
    if !candidate.is_zero_d() {
        return;
    }
    let center = scott_science::center_of_mass(candidate);
    for coord in &mut candidate.fractional_coords {
        coord[0] -= center[0];
        coord[1] -= center[1];
        coord[2] -= center[2];
    }

    let eig = SymmetricEigen::new(scott_science::inertia_tensor(candidate));
    let (sorted_values, sorted_vectors) = sort_eigensystem(
        [eig.eigenvalues[0], eig.eigenvalues[1], eig.eigenvalues[2]],
        eig.eigenvectors,
    );
    let _ = sorted_values;
    rotate_to_principal_axes(candidate, &sorted_vectors);
}

fn sort_eigensystem(values: [f64; 3], vectors: Matrix3<f64>) -> ([f64; 3], Matrix3<f64>) {
    let mut order = [
        (0usize, values[0]),
        (1usize, values[1]),
        (2usize, values[2]),
    ];
    order.sort_by(|left, right| left.1.partial_cmp(&right.1).unwrap_or(Ordering::Equal));
    let mut sorted_values = [0.0; 3];
    let mut sorted_vectors = Matrix3::<f64>::zeros();
    for (col, (src_idx, value)) in order.into_iter().enumerate() {
        sorted_values[col] = value;
        sorted_vectors.set_column(col, &vectors.column(src_idx));
    }
    (sorted_values, sorted_vectors)
}

fn rotate_to_principal_axes(candidate: &mut Candidate, eigenvectors: &Matrix3<f64>) {
    for coord in &mut candidate.fractional_coords {
        let rotated = Vector3::new(coord[0], coord[1], coord[2]).transpose() * *eigenvectors;
        coord[0] = rotated[(0, 0)];
        coord[1] = rotated[(0, 1)];
        coord[2] = rotated[(0, 2)];
    }
}

pub(crate) fn rotate_candidate_angles(candidate: &mut Candidate, phi: f64, eta: f64) {
    if !candidate.is_zero_d() {
        return;
    }
    let center = center_of_geometry(&candidate.fractional_coords);
    let cos_phi = phi.cos();
    let sin_phi = phi.sin();
    let cos_eta = eta.cos();
    let sin_eta = eta.sin();
    for coord in &mut candidate.fractional_coords {
        let x_old = coord[0] - center[0];
        let y_old = coord[1] - center[1];
        let z_old = coord[2] - center[2];
        let xy_old = sin_eta * x_old + cos_eta * y_old;
        coord[0] = center[0] + cos_eta * x_old - sin_eta * y_old;
        coord[1] = center[1] + cos_phi * xy_old - sin_phi * z_old;
        coord[2] = center[2] + sin_phi * xy_old + cos_phi * z_old;
    }
}

pub(crate) fn rotate_candidate_z_axis(candidate: &mut Candidate, phi: f64) {
    if !candidate.is_zero_d() {
        return;
    }
    let cos_phi = phi.cos();
    let sin_phi = phi.sin();
    for coord in &mut candidate.fractional_coords {
        let x_old = coord[0];
        let y_old = coord[1];
        coord[0] = cos_phi * x_old - sin_phi * y_old;
        coord[1] = sin_phi * x_old + cos_phi * y_old;
    }
}

pub(crate) fn two_pi(value: f64) -> f64 {
    std::f64::consts::TAU * value
}

fn distance_sq(left: [f64; 3], right: [f64; 3]) -> f64 {
    let dx = left[0] - right[0];
    let dy = left[1] - right[1];
    let dz = left[2] - right[2];
    dx * dx + dy * dy + dz * dz
}

#[derive(Debug, Clone)]
pub(crate) struct TinyRng {
    state: u64,
}

impl TinyRng {
    pub(crate) fn new(seed: u64) -> Self {
        Self {
            state: seed.wrapping_add(0x9E37_79B9_7F4A_7C15),
        }
    }

    pub(crate) fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1);
        self.state
    }

    pub(crate) fn next_f64(&mut self) -> f64 {
        let bits = self.next_u64() >> 11;
        (bits as f64) / ((1u64 << 53) as f64)
    }

    pub(crate) fn next_usize(&mut self, upper: usize) -> usize {
        if upper <= 1 {
            0
        } else {
            (self.next_u64() as usize) % upper
        }
    }
}
