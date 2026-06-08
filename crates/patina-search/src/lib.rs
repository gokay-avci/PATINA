#![forbid(unsafe_code)]

/*!
What this crate implements: lightweight Rust-side BH and GA control logic that can drive any
backend through the shared candidate/evaluation types.
Design basis: prompt Sections 2, 3, 6, and 10 place algorithm control in Rust while keeping
evaluation behind an isolated backend seam and treating failed evaluations as recoverable.
Assumption: the first integrated control-plane version prioritizes deterministic, testable search
machinery over full scientific parity with legacy KLMC3 internals.
*/

use patina_types::Candidate;
#[cfg(test)]
use patina_types::{EvalResult, Population, SearchConfig};
use std::fmt;

mod candidate_ops;
mod controllers;
mod operator_kernels;
mod scott_ga_controller;
mod scott_ga_parity;
pub mod scott_kernels;
mod scott_science;
mod workflow_kernel;
mod workflow_snapshots;

#[cfg(test)]
use candidate_ops::cluster_intrinsic_dims;
pub(crate) use candidate_ops::{
    assert_native_supported_search_periodicity, mutate_candidate, random_foreign_structure,
    ScottGeometryModel, TinyRng,
};
pub(crate) use candidate_ops::{
    geometry_is_reasonable, perturb_candidate, rotate_candidate_angles,
    standardize_candidate_coordinates, two_pi,
};
pub use controllers::{
    BasinHopping, BhMethod, BhMoveClass, BhMoveClassPolicy, BhStepTrace, EnergyLid,
    GeneticAlgorithm, McStepTrace, MonteCarlo, SearchAlgorithm, SearchUpdate, SimulatedAnnealing,
};
pub(crate) use operator_kernels::{
    cluster_crossover_candidate_3d, crossover_candidate, naive_split_crossover,
    self_crossover_candidate,
};
pub use scott_ga_parity::{
    classify_duplicate_candidates, classify_duplicate_candidates_after_hashkey_probe,
    DuplicateCandidateSnapshot, DuplicatePolicy, ScottDuplicateClassifier, ScottDuplicateReason,
    ScottGaGenerationTrace, ScottGaMember, ScottGaOperatorConfig, ScottGaOrigin,
    ScottGaPendingRepopulation, ScottGaPopulationTransition, ScottGaStage, ScottGaTopologyIdentity,
    ScottParityGeneticAlgorithm,
};
#[cfg(test)]
use scott_kernels::apply_bh_move_class;
pub use scott_kernels::{
    accept_bh_energy_comparison, accept_energy_transition, advance_annealing_schedule,
    advance_bh_step_control, advance_energy_lid_schedule, annealing_acceptance,
    annealing_final_temperature_after_steps, annealing_quench_seed_lineage,
    annealing_quench_step_lineage, annealing_sampling_schedule, annealing_seed_lineage,
    annealing_step_lineage, apply_production_data_mining, apply_production_data_mining_to_species,
    build_hybrid_crossover_attempt_record, build_production_data_mining_plan,
    build_walker_restart_equivalence, choose_bh_move_class_from_draw, decide_bh_acceptance,
    decide_bh_acceptance_from_comparison, energy_lid_acceptance, energy_lid_runner_seed_lineage,
    energy_lid_runner_step_lineage, energy_lid_sampling_schedule, energy_lid_seed_lineage,
    energy_lid_step_lineage, energy_lid_threshold_for_lid, enforce_production_master_species,
    extract_master_species_from_atom_block, hybrid_child_origin_from_label,
    initial_annealing_schedule, initial_energy_lid_schedule, metropolis_acceptance,
    run_fixed_parent_hybrid_crossover, AcceptanceDecision, AnnealingScheduleConfig,
    AnnealingScheduleState, BhEnergyComparison, BhStepControlState, EnergyLidScheduleConfig,
    EnergyLidScheduleState, FixedParentHybridCrossoverConfig, HybridAcceptedChild,
    HybridCrossoverAttemptInput, MonteCarloAcceptance, ProductionAtomSpec, ProductionBestEntry,
    ProductionBestSet, ProductionBestSetConfig, ProductionBestSetDecision, ProductionBestSetMatch,
    ProductionDataMiningPlan, ProductionTransformError,
};
pub use scott_science::{
    assess_candidate_geometry, assess_cluster_geometry, assess_periodic_cell,
    build_structure_edges, cartesian_to_fractional, classify_topology_verdict,
    compute_structure_hashkey_radius, fractional_to_cartesian, lattice_params_from_vectors,
    lattice_vectors_from_params, minimum_image_cartesian_distance_sq,
    minimum_image_cartesian_distance_sq_with_axes, nearest_periodic_image_fractional,
    nearest_periodic_image_fractional_with_axes, normalize_fractional_coordinate,
    normalize_fractional_coordinate_with_axes, normalize_periodic_candidate,
    periodic_neighbors_within_cutoff, periodic_neighbors_within_cutoff_with_axes,
    summarize_coordination_histograms, summarize_edge_pairs, AtomSpec, CellAssessment,
    GeometryAssessment, GeometryRejectionReason, HashkeyRadiusMode, ScottScienceError,
    TopologyAtom, UnitCellParameters,
};
pub use workflow_kernel::{
    MonteCarloKernelState, PopulationKernelState, ProductionKernelState, SamplingSchedule,
    WorkflowEvaluationTask, WorkflowFamily, WorkflowKernelError, WorkflowLineage, WorkflowMember,
    WorkflowPhase, WorkflowRejection,
};
pub use workflow_snapshots::{
    build_energy_lid_window_state, build_simulated_annealing_structure_state,
};

pub type SearchResult<T> = Result<T, SearchLifecycleError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchLifecycleError {
    NotInitialized {
        controller: &'static str,
        operation: &'static str,
    },
}

impl SearchLifecycleError {
    pub(crate) fn not_initialized(controller: &'static str, operation: &'static str) -> Self {
        Self::NotInitialized {
            controller,
            operation,
        }
    }
}

impl fmt::Display for SearchLifecycleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotInitialized {
                controller,
                operation,
            } => write!(
                f,
                "{controller} cannot run `{operation}` before it has been initialized"
            ),
        }
    }
}

impl std::error::Error for SearchLifecycleError {}

pub fn scott_intent_pmoi(candidate: &Candidate) -> [f64; 3] {
    scott_science::scott_intent_pmoi(candidate)
}

pub fn scott_intent_pmoi_difference(left: &Candidate, right: &Candidate) -> f64 {
    scott_science::scott_intent_pmoi_difference(left, right)
}

pub(crate) fn compare_pmoi_scott_intent(
    left: &Candidate,
    right: &Candidate,
    tolerance: f64,
) -> bool {
    scott_science::compare_pmoi_scott_intent(left, right, tolerance)
}

#[cfg(test)]
fn normalized_pmoi(candidate: &Candidate) -> [f64; 3] {
    scott_intent_pmoi(candidate)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod workflow_kernel_tests;
