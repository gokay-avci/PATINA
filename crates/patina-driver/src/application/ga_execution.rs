use serde::Serialize;
use std::collections::BTreeMap;

use patina_search::PopulationKernelState;

#[derive(Debug, Clone, Serialize)]
pub struct RustJanusGenerationArtifact {
    pub generation: usize,
    pub phase: String,
    pub request_count: usize,
    pub success_count: usize,
    pub failure_count: usize,
    pub failure_kind_counts: BTreeMap<String, usize>,
    pub converged_count: usize,
    pub elapsed_secs: f64,
    pub best_energy: Option<f64>,
    pub mean_energy: Option<f64>,
    pub worst_energy: Option<f64>,
    pub boundary_population_size: usize,
    pub boundary_valid_population_size: usize,
    pub population_size: usize,
    pub valid_population_size: usize,
    pub duplicate_count: usize,
    pub duplicate_hashkey_count: usize,
    pub duplicate_pmoi_count: usize,
    pub duplicate_energy_tol_count: usize,
    pub repopulated_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct RustJanusOriginMetricRow {
    pub generation: usize,
    pub phase: String,
    pub origin: String,
    pub request_count: usize,
    pub success_count: usize,
    pub converged_count: usize,
    pub survivor_count: usize,
    pub working_survivor_count: usize,
}

#[derive(Debug, Clone)]
pub struct RustJanusGaExecution {
    pub generation_artifacts: Vec<RustJanusGenerationArtifact>,
    pub generation_responses: Vec<Vec<patina_types::WorkerResponse>>,
    pub generation_origin_metrics: Vec<Vec<RustJanusOriginMetricRow>>,
    pub generation_boundary_populations: Vec<Vec<patina_search::ScottGaMember>>,
    pub generation_populations: Vec<Vec<patina_search::ScottGaMember>>,
    pub generation_boundary_kernel_states: Vec<PopulationKernelState>,
    pub generation_kernel_states: Vec<PopulationKernelState>,
    pub controller_trace: Vec<patina_search::ScottGaGenerationTrace>,
    pub final_population: Vec<patina_search::ScottGaMember>,
}
