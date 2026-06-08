use crate::{
    cluster_crossover_candidate_3d, perturb_candidate, rotate_candidate_angles,
    standardize_candidate_coordinates, two_pi, BhEnergyComparison, BhMoveClass, BhMoveClassPolicy,
    MonteCarloAcceptance, ScottGeometryModel, TinyRng,
};
use patina_types::Candidate;

use super::{accept_bh_energy_comparison, accept_energy_transition, AcceptanceDecision};

/// Compact BH controller-local state for pure step-control decisions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BhStepControlState {
    pub rejection_counter: usize,
    pub random_moveclass: bool,
    pub current_step_size: f64,
}

/// One sampled BH move plan produced from the current move regime.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BhMovePlan {
    pub move_class: BhMoveClass,
    pub step_size: f64,
}

pub fn advance_bh_step_control(
    state: BhStepControlState,
    base_step_size: f64,
    policy: BhMoveClassPolicy,
) -> BhStepControlState {
    let mut next = state;
    if next.rejection_counter > policy.dynamic_step_threshold {
        next.rejection_counter = 0;
        next.current_step_size += base_step_size;
    }

    if next.rejection_counter > policy.moveclass_threshold
        || next.current_step_size > policy.max_dynamic_step_multiplier * base_step_size
    {
        next.current_step_size = base_step_size;
        next.random_moveclass = true;
    }
    next
}

pub fn finalize_bh_step_control(
    state: BhStepControlState,
    accepted_any: bool,
    base_step_size: f64,
) -> BhStepControlState {
    if accepted_any {
        BhStepControlState {
            rejection_counter: 0,
            random_moveclass: false,
            current_step_size: base_step_size,
        }
    } else {
        BhStepControlState {
            rejection_counter: state.rejection_counter + 1,
            ..state
        }
    }
}

pub fn choose_bh_move_class_from_draw(
    random_moveclass: bool,
    policy: BhMoveClassPolicy,
    draw: f64,
) -> BhMoveClass {
    if !random_moveclass {
        return BhMoveClass::MonteCarlo;
    }

    let mut threshold = 1.0 - policy.prob_swap_cations;
    if draw > threshold {
        BhMoveClass::SwapCations
    } else {
        threshold -= policy.prob_swap_atoms;
        if draw > threshold {
            BhMoveClass::SwapAtoms
        } else {
            threshold -= policy.prob_mutate_cluster;
            if draw > threshold {
                BhMoveClass::MutateCluster
            } else {
                threshold -= policy.prob_twist_cluster;
                if draw > threshold {
                    BhMoveClass::TwistCluster
                } else {
                    threshold -= policy.prob_translate_surface;
                    if draw > threshold {
                        BhMoveClass::TranslateCluster
                    } else {
                        threshold -= policy.prob_rotate_surface;
                        if draw > threshold {
                            BhMoveClass::RotateCluster
                        } else {
                            BhMoveClass::MonteCarlo
                        }
                    }
                }
            }
        }
    }
}

pub fn sample_bh_move_plan(
    state: BhStepControlState,
    policy: BhMoveClassPolicy,
    draw: f64,
) -> BhMovePlan {
    BhMovePlan {
        move_class: choose_bh_move_class_from_draw(state.random_moveclass, policy, draw),
        step_size: state.current_step_size,
    }
}

pub fn decide_bh_acceptance(
    current_energy: Option<f64>,
    candidate_energy: f64,
    mode: MonteCarloAcceptance,
    random_draw: f64,
) -> AcceptanceDecision {
    accept_energy_transition(current_energy, candidate_energy, mode, random_draw)
}

pub fn decide_bh_acceptance_from_comparison(
    comparison: BhEnergyComparison,
    mode: MonteCarloAcceptance,
    random_draw: f64,
) -> AcceptanceDecision {
    accept_bh_energy_comparison(comparison, mode, random_draw)
}

pub(crate) fn apply_bh_move_class(
    candidate: &mut Candidate,
    move_class: BhMoveClass,
    step_size: f64,
    rng: &mut TinyRng,
) {
    match move_class {
        BhMoveClass::MonteCarlo => perturb_candidate(candidate, step_size, rng),
        BhMoveClass::SwapCations => swap_candidate_cation_positions(candidate, rng),
        BhMoveClass::SwapAtoms => swap_candidate_atom_positions(candidate, rng),
        BhMoveClass::MutateCluster => mutate_cluster_bh(candidate, step_size, rng),
        BhMoveClass::TwistCluster => twist_cluster_bh(candidate, rng),
        BhMoveClass::TranslateCluster => translate_cluster_bh(candidate, step_size, rng),
        BhMoveClass::RotateCluster => rotate_cluster_bh(candidate, rng),
    }
}

fn swap_candidate_atom_positions(candidate: &mut Candidate, rng: &mut TinyRng) {
    let len = candidate.len();
    if len <= 1 {
        return;
    }
    for i in 0..len {
        if rng.next_f64() > 0.8 {
            let j = rng.next_usize(len);
            candidate.fractional_coords.swap(i, j);
        }
    }
}

fn swap_candidate_cation_positions(candidate: &mut Candidate, rng: &mut TinyRng) {
    let Some(first_species) = candidate.species.first() else {
        return;
    };
    let cation_count = candidate
        .species
        .iter()
        .take_while(|species| *species == first_species)
        .count();
    if cation_count <= 1 {
        return;
    }

    for i in 0..cation_count {
        if rng.next_f64() > 0.8 {
            let j = rng.next_usize(cation_count);
            candidate.fractional_coords.swap(i, j);
        }
    }
}

fn mutate_cluster_bh(candidate: &mut Candidate, step_size: f64, rng: &mut TinyRng) {
    let rotated_left = {
        let mut copy = candidate.clone();
        rotate_candidate_angles(&mut copy, two_pi(rng.next_f64()), two_pi(rng.next_f64()));
        copy
    };
    let rotated_right = {
        let mut copy = candidate.clone();
        rotate_candidate_angles(&mut copy, two_pi(rng.next_f64()), two_pi(rng.next_f64()));
        copy
    };
    let mut child = cluster_crossover_candidate_3d(
        &rotated_left,
        &rotated_right,
        rng,
        &ScottGeometryModel::from_base(candidate),
    );
    perturb_candidate(&mut child, step_size * 0.5, rng);
    *candidate = child;
}

fn twist_cluster_bh(candidate: &mut Candidate, rng: &mut TinyRng) {
    if candidate.len() <= 1 {
        return;
    }
    let mut rotated = candidate.clone();
    rotate_candidate_angles(&mut rotated, two_pi(rng.next_f64()), two_pi(rng.next_f64()));
    let (z_min, z_max) = rotated.fractional_coords.iter().fold(
        (f64::INFINITY, f64::NEG_INFINITY),
        |(min_v, max_v), coord| (min_v.min(coord[2]), max_v.max(coord[2])),
    );
    let cut = z_min + (0.1 + 0.8 * rng.next_f64()) * (z_max - z_min).abs();
    let psi = two_pi(rng.next_f64());
    let cos_psi = psi.cos();
    let sin_psi = psi.sin();
    for coord in &mut rotated.fractional_coords {
        if coord[2] < cut {
            let x = coord[0];
            let y = coord[1];
            coord[0] = cos_psi * x - sin_psi * y;
            coord[1] = sin_psi * x + cos_psi * y;
        }
    }
    standardize_candidate_coordinates(&mut rotated);
    *candidate = rotated;
}

fn translate_cluster_bh(candidate: &mut Candidate, step_size: f64, rng: &mut TinyRng) {
    let shift = [
        2.0 * step_size * (rng.next_f64() - 0.5),
        2.0 * step_size * (rng.next_f64() - 0.5),
        2.0 * step_size * (rng.next_f64() - 0.5),
    ];
    for coord in &mut candidate.fractional_coords {
        coord[0] += shift[0];
        coord[1] += shift[1];
        coord[2] += shift[2];
    }
}

fn rotate_cluster_bh(candidate: &mut Candidate, rng: &mut TinyRng) {
    rotate_candidate_angles(candidate, two_pi(rng.next_f64()), two_pi(rng.next_f64()));
}
