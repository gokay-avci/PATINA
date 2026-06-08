use std::collections::BTreeMap;
use std::path::PathBuf;

use patina_types::StructureRecord;
use serde::Deserialize;

#[derive(Debug, Clone, Default)]
pub struct GenerationStore {
    pub summaries: BTreeMap<usize, GenerationSummaryRow>,
    pub states: BTreeMap<usize, GenerationStateFile>,
}

// TUI read models preserve the full on-disk artifact schema even when the
// current screens only surface a subset of fields.
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct GenerationSummaryRow {
    pub generation: usize,
    pub phase: String,
    pub request_count: usize,
    pub success_count: usize,
    pub failure_count: usize,
    pub converged_count: usize,
    pub elapsed_secs: f64,
    pub best_energy: Option<f64>,
    pub mean_energy: Option<f64>,
    pub worst_energy: Option<f64>,
    pub population_size: usize,
    pub valid_population_size: usize,
    #[serde(default)]
    pub duplicate_count: usize,
    #[serde(default)]
    pub duplicate_hashkey_count: usize,
    #[serde(default)]
    pub duplicate_pmoi_count: usize,
    #[serde(default)]
    pub duplicate_energy_tol_count: usize,
    #[serde(default)]
    pub repopulated_count: usize,
}

// TUI read models preserve the full on-disk artifact schema even when the
// current screens only surface a subset of fields.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct GenerationMetricRow {
    pub generation: usize,
    pub phase: String,
    pub elapsed_secs: f64,
    pub request_count: usize,
    pub success_count: usize,
    pub failure_count: usize,
    pub converged_count: usize,
    pub best_energy: Option<f64>,
    pub mean_energy: Option<f64>,
    pub worst_energy: Option<f64>,
    pub population_size: Option<usize>,
    pub valid_population_size: Option<usize>,
    pub duplicate_count: Option<usize>,
    pub repopulated_count: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct ControllerTraceRow {
    pub generation: usize,
    pub stage: String,
    pub population_size: usize,
    pub valid_population_size: usize,
    pub child_count: usize,
    pub duplicate_count: usize,
    pub duplicate_hashkey_count: usize,
    pub duplicate_pmoi_count: usize,
    pub duplicate_energy_tol_count: usize,
    pub repopulated_count: usize,
    pub best_energy: Option<f64>,
    pub selected_indices: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct ArtifactIndex {
    pub manifest_path: PathBuf,
    pub raw_files: Vec<PathBuf>,
    pub output_files: Vec<PathBuf>,
}

// TUI read models preserve the full on-disk artifact schema even when the
// current screens only surface a subset of fields.
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct GenerationStateFile {
    pub generation: usize,
    pub population: Vec<GenerationMemberFile>,
    #[serde(default)]
    pub elites: Vec<GenerationMemberFile>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GenerationMemberFile {
    pub member_id: usize,
    pub origin: String,
    pub occurrences: usize,
    pub source: StructureRecord,
    pub evaluation: EvaluationRecordFile,
}

// TUI read models preserve the full on-disk artifact schema even when the
// current screens only surface a subset of fields.
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct EvaluationRecordFile {
    pub label: String,
    pub energy: Option<f64>,
    pub converged: bool,
    pub structure: StructureRecord,
    #[serde(default)]
    pub backend_run_dir: Option<String>,
    #[serde(default)]
    pub primary_output_path: Option<String>,
}
