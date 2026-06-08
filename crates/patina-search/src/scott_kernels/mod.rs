//! Scott-specific deterministic kernels that belong inside the Rust scientific core.
//!
//! This module tree is the landing zone for workflow-family scientific rules that should not be
//! modeled as outbound ports or scattered across controller adapters.

mod acceptance;
mod annealing;
mod best_set;
mod energy_lid;
mod hybrid;
mod moves;
mod population;
mod production_transforms;
mod restart;

pub use acceptance::{
    accept_bh_energy_comparison, accept_energy_transition, metropolis_acceptance,
    AcceptanceDecision, BhEnergyComparison, MonteCarloAcceptance,
};
pub use annealing::{
    advance_annealing_schedule, annealing_acceptance, annealing_final_temperature_after_steps,
    annealing_quench_seed_lineage, annealing_quench_step_lineage, annealing_sampling_schedule,
    annealing_seed_lineage, annealing_step_lineage, initial_annealing_schedule,
    AnnealingScheduleConfig, AnnealingScheduleState,
};
pub use best_set::{
    ProductionBestEntry, ProductionBestSet, ProductionBestSetConfig, ProductionBestSetDecision,
    ProductionBestSetMatch,
};
pub use energy_lid::{
    advance_energy_lid_schedule, energy_lid_acceptance, energy_lid_runner_seed_lineage,
    energy_lid_runner_step_lineage, energy_lid_sampling_schedule, energy_lid_seed_lineage,
    energy_lid_step_lineage, energy_lid_threshold_for_lid, initial_energy_lid_schedule,
    EnergyLidScheduleConfig, EnergyLidScheduleState,
};
pub use hybrid::{
    build_hybrid_crossover_attempt_record, hybrid_child_origin_from_label,
    run_fixed_parent_hybrid_crossover, FixedParentHybridCrossoverConfig, HybridAcceptedChild,
    HybridCrossoverAttemptInput,
};
pub use moves::{
    advance_bh_step_control, choose_bh_move_class_from_draw, decide_bh_acceptance,
    decide_bh_acceptance_from_comparison, BhStepControlState,
};
pub(crate) use moves::{apply_bh_move_class, finalize_bh_step_control, sample_bh_move_plan};
pub use production_transforms::{
    apply_production_data_mining, apply_production_data_mining_to_species,
    build_production_data_mining_plan, enforce_production_master_species,
    extract_master_species_from_atom_block, ProductionAtomSpec, ProductionDataMiningPlan,
    ProductionTransformError,
};
pub use restart::build_walker_restart_equivalence;
