use crate::candidate_ops::{
    assert_native_supported_search_periodicity, candidate_geometry_score, cluster_intrinsic_dims,
    geometry_is_reasonable, move_coords_to_com, mutate_candidate, periodic_axis_indices,
    rotate_candidate_angles, rotate_candidate_z_axis, standardize_candidate_coordinates, two_pi,
    ScottGeometryModel, TinyRng,
};
use crate::ScottGaOperatorConfig;
use patina_types::Candidate;

/// Genetic-operator kernels that shape child candidates before evaluation.
///
/// These remain internal because they are controller implementation details rather than
/// crate-level workflow contracts.
pub(crate) fn crossover_candidate(
    left: &Candidate,
    right: &Candidate,
    rng: &mut TinyRng,
    operator_cfg: ScottGaOperatorConfig,
    geometry_model: &ScottGeometryModel,
) -> Candidate {
    assert_native_supported_search_periodicity(left, "crossover_candidate left");
    assert_native_supported_search_periodicity(right, "crossover_candidate right");
    if left.len() != right.len() || left.species.len() != right.species.len() {
        return naive_split_crossover(left, right, rng);
    }

    if left.is_zero_d() {
        let mut best = naive_split_crossover(left, right, rng);
        standardize_candidate_coordinates(&mut best);
        let mut best_score = candidate_geometry_score(&best, geometry_model);
        let dim1 = cluster_intrinsic_dims(left, operator_cfg.dim_tolerance);
        let dim2 = cluster_intrinsic_dims(right, operator_cfg.dim_tolerance);
        let use_1d_2d =
            dim1.max(dim2) < 3 || rng.next_f64() <= operator_cfg.cross_1d_2d_ratio.clamp(0.0, 1.0);
        for _ in 0..operator_cfg.crossover_attempts.max(1) {
            let candidate = if use_1d_2d {
                cluster_crossover_candidate_1d_2d(
                    left,
                    right,
                    dim1,
                    dim2,
                    rng,
                    operator_cfg.cluster_crossover_fragment_offset_z,
                    geometry_model,
                )
            } else {
                cluster_crossover_candidate_3d(left, right, rng, geometry_model)
            };
            let score = candidate_geometry_score(&candidate, geometry_model);
            if score > best_score {
                best = candidate;
                best_score = score;
            }
        }
        return best;
    }

    let periodic_axes = periodic_axis_indices(left);
    let axis = if periodic_axes.is_empty() {
        0
    } else {
        periodic_axes[rng.next_usize(periodic_axes.len())]
    };
    let mut axis_min = f64::INFINITY;
    let mut axis_max = f64::NEG_INFINITY;
    for coord in &right.fractional_coords {
        axis_min = axis_min.min(coord[axis]);
        axis_max = axis_max.max(coord[axis]);
    }
    let span = (axis_max - axis_min).abs();
    if span <= 1.0e-12 {
        return naive_split_crossover(left, right, rng);
    }
    let cut = axis_min + (0.1 + 0.8 * rng.next_f64()) * span;

    let mut child = left.clone();
    let mut available = vec![true; right.len()];
    for idx in 0..left.len() {
        if left.fractional_coords[idx][axis] >= cut {
            continue;
        }
        let species = &left.species[idx];
        let mut replacement = None;
        let mut best_axis = f64::INFINITY;
        for (j, is_available) in available.iter().enumerate().take(right.len()) {
            if !*is_available || &right.species[j] != species {
                continue;
            }
            let candidate_axis = right.fractional_coords[j][axis];
            if candidate_axis <= best_axis {
                best_axis = candidate_axis;
                replacement = Some(j);
            }
        }
        if let Some(j) = replacement {
            available[j] = false;
            child.fractional_coords[idx] = right.fractional_coords[j];
        }
    }

    standardize_candidate_coordinates(&mut child);
    child
}

pub(crate) fn cluster_crossover_candidate_3d(
    left: &Candidate,
    right: &Candidate,
    rng: &mut TinyRng,
    geometry_model: &ScottGeometryModel,
) -> Candidate {
    let mut left_rot = left.clone();
    let mut right_rot = right.clone();
    rotate_candidate_angles(
        &mut left_rot,
        two_pi(rng.next_f64()),
        two_pi(rng.next_f64()),
    );
    rotate_candidate_angles(
        &mut right_rot,
        two_pi(rng.next_f64()),
        two_pi(rng.next_f64()),
    );
    standardize_candidate_coordinates(&mut left_rot);
    standardize_candidate_coordinates(&mut right_rot);

    let (z_min, z_max) = right_rot.fractional_coords.iter().fold(
        (f64::INFINITY, f64::NEG_INFINITY),
        |(min_v, max_v), coord| (min_v.min(coord[2]), max_v.max(coord[2])),
    );
    let span = (z_max - z_min).abs();
    if span <= 1.0e-12 {
        return naive_split_crossover(left, right, rng);
    }
    let mut child = left_rot.clone();
    let cut = z_min + (0.1 + 0.8 * rng.next_f64()) * span;
    let mut available = vec![true; right_rot.len()];

    for idx in 0..child.len() {
        if child.fractional_coords[idx][2] >= cut {
            continue;
        }
        let species = &left_rot.species[idx];
        let mut replacement = None;
        let mut lowest_z = f64::INFINITY;
        for (j, is_available) in available.iter().enumerate().take(right_rot.len()) {
            if !*is_available || &right_rot.species[j] != species {
                continue;
            }
            let z = right_rot.fractional_coords[j][2];
            if z < lowest_z {
                lowest_z = z;
                replacement = Some(j);
            }
        }
        if let Some(j) = replacement {
            available[j] = false;
            child.fractional_coords[idx] = right_rot.fractional_coords[j];
        }
    }

    standardize_candidate_coordinates(&mut child);
    if !geometry_is_reasonable(&child, geometry_model) {
        return naive_split_crossover(left, right, rng);
    }
    child
}

fn cluster_crossover_candidate_1d_2d(
    left: &Candidate,
    right: &Candidate,
    left_dim: usize,
    right_dim: usize,
    rng: &mut TinyRng,
    fragment_offset_z: f64,
    geometry_model: &ScottGeometryModel,
) -> Candidate {
    let mut left_rot = left.clone();
    let mut right_rot = right.clone();
    rotate_candidate_z_axis(&mut left_rot, two_pi(rng.next_f64()));
    rotate_candidate_z_axis(&mut right_rot, two_pi(rng.next_f64()));

    let axis = if left_dim.max(right_dim) <= 1 || rng.next_f64() < 0.5 {
        0
    } else {
        1
    };

    let mut axis_min = f64::INFINITY;
    let mut axis_max = f64::NEG_INFINITY;
    for coord in &right_rot.fractional_coords {
        axis_min = axis_min.min(coord[axis]);
        axis_max = axis_max.max(coord[axis]);
    }
    let span = (axis_max - axis_min).abs();
    if span <= 1.0e-12 {
        return naive_split_crossover(left, right, rng);
    }
    let cut = axis_min + (0.1 + 0.8 * rng.next_f64()) * span;

    let mut temp1_coords = Vec::with_capacity(left.len());
    let mut temp1_species = Vec::with_capacity(left.len());
    let mut temp2_coords = Vec::with_capacity(left.len());
    let mut temp2_species = Vec::with_capacity(left.len());
    let mut available = vec![true; right_rot.len()];

    for idx in 0..left_rot.len() {
        let coord_value = left_rot.fractional_coords[idx][axis];
        if coord_value >= cut {
            temp1_coords.push(left_rot.fractional_coords[idx]);
            temp1_species.push(left_rot.species[idx].clone());
            continue;
        }

        let species = &left_rot.species[idx];
        let mut replacement = None;
        let mut lowest_axis = f64::INFINITY;
        for (j, is_available) in available.iter().enumerate().take(right_rot.len()) {
            if !*is_available || &right_rot.species[j] != species {
                continue;
            }
            let right_value = right_rot.fractional_coords[j][axis];
            if right_value <= lowest_axis {
                lowest_axis = right_value;
                replacement = Some(j);
            }
        }

        if let Some(j) = replacement {
            available[j] = false;
            temp2_coords.push(right_rot.fractional_coords[j]);
            temp2_species.push(left_rot.species[idx].clone());
        } else {
            return naive_split_crossover(left, right, rng);
        }
    }

    move_coords_to_com(&mut temp1_coords);
    move_coords_to_com(&mut temp2_coords);

    let mut child = left.clone();
    let mut write_idx = 0;
    for (species, coord) in temp1_species.into_iter().zip(temp1_coords) {
        child.species[write_idx] = species;
        child.fractional_coords[write_idx] = coord;
        write_idx += 1;
    }
    for (species, mut coord) in temp2_species.into_iter().zip(temp2_coords) {
        coord[2] += fragment_offset_z;
        child.species[write_idx] = species;
        child.fractional_coords[write_idx] = coord;
        write_idx += 1;
    }

    standardize_candidate_coordinates(&mut child);
    if !geometry_is_reasonable(&child, geometry_model) {
        return naive_split_crossover(left, right, rng);
    }
    child
}

pub(crate) fn self_crossover_candidate(
    candidate: &Candidate,
    step_size: f64,
    rng: &mut TinyRng,
    operator_cfg: ScottGaOperatorConfig,
) -> Candidate {
    let mut child = candidate.clone();
    let len = child.len();
    if len > 1 {
        let window = (len / 4).max(1);
        let start = rng.next_usize(len);
        for offset in 0..window {
            let left = (start + offset) % len;
            let right = (start + window + offset) % len;
            child.fractional_coords.swap(left, right);
        }
    }
    let moves = 1;
    mutate_candidate(
        &mut child,
        moves,
        (step_size * 0.18).max(0.03),
        rng,
        operator_cfg,
    );
    child
}

pub(crate) fn naive_split_crossover(
    left: &Candidate,
    right: &Candidate,
    rng: &mut TinyRng,
) -> Candidate {
    let mut child = left.clone();
    let split = rng.next_usize(left.len().max(1));
    for idx in split..child.len() {
        child.species[idx] = right.species[idx].clone();
        child.fractional_coords[idx] = right.fractional_coords[idx];
    }
    child
}
