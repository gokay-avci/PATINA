use patina_types::{EvaluationRecord, StructureRecord};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize)]
pub struct AppOverview {
    pub app_name: String,
    pub campaign_checkpoint: String,
    pub reference_workspace_root: String,
    pub local_first_claims: Vec<String>,
    pub stack: Vec<StackDecision>,
    pub saint_translation: Vec<SaintTranslation>,
    pub phases: Vec<PhaseStatus>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StackDecision {
    pub area: String,
    pub choice: String,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SaintTranslation {
    pub saint_shape: String,
    pub patina_direction: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PhaseStatus {
    pub name: String,
    pub status: String,
    pub objective: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DatabaseProfile {
    pub engine: String,
    pub namespace: String,
    pub database: String,
    pub path: String,
    pub connected: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DataFoundation {
    pub thesis: String,
    pub authority_zones: Vec<AuthorityZone>,
    pub sync_principles: Vec<SyncPrinciple>,
    pub table_blueprint: Vec<TableBlueprint>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuthorityZone {
    pub zone: String,
    pub authority: String,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncPrinciple {
    pub name: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TableBlueprint {
    pub table: String,
    pub role: String,
    pub authority: String,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceCatalog {
    pub runs_root: String,
    pub discovered: usize,
    pub runs: Vec<WorkspaceRunCard>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceRunCard {
    pub run_name: String,
    pub path: String,
    pub catalog_status: String,
    pub catalog_notes: Vec<String>,
    pub has_manifest: bool,
    pub has_checkpoint: bool,
    pub workflow_owner: Option<String>,
    pub workflow_scope: Option<String>,
    pub system: Option<String>,
    pub backend: Option<String>,
    pub runtime_engine: Option<String>,
    pub lane_mode: Option<String>,
    pub run_spec: RunSpecDigest,
    pub ga_live: Option<GaLiveDigest>,
    pub ga_documents: Vec<GaRunDocument>,
    pub ga_generations: Vec<GaGenerationObservation>,
    pub top_candidates: Vec<GaTopCandidate>,
    pub ga_provenance_notes: Vec<String>,
    pub best_structure: Option<StructureDigest>,
    pub structure_preview: Option<StructurePreview>,
    pub artifacts: ArtifactDigest,
    #[serde(skip_serializing)]
    pub manifest_payload: Option<Value>,
    #[serde(skip_serializing)]
    pub best_structure_record: Option<StructureRecord>,
    #[serde(skip_serializing)]
    pub best_evaluation_record: Option<EvaluationRecord>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GaRunDocument {
    pub role: String,
    pub path: String,
    pub status: String,
    pub detail: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RunSpecDigest {
    pub population_size: Option<usize>,
    pub requested_generations: Option<usize>,
    pub seed: Option<u64>,
    pub temperature: Option<f64>,
    pub step_size: Option<f64>,
    pub parallel_contract: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GaLiveDigest {
    pub current_generation: usize,
    pub population_size: usize,
    pub elite_count: usize,
    pub converged_count: usize,
    pub repopulation_count: usize,
    pub best_energy: Option<f64>,
    pub mean_energy: Option<f64>,
    pub worst_energy: Option<f64>,
    pub updated_at_unix_ms: Option<u64>,
    pub history: Vec<GaGenerationPoint>,
    pub population: Vec<GaPopulationMember>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GaGenerationPoint {
    pub generation: usize,
    pub population_size: usize,
    pub elite_count: usize,
    pub converged_count: usize,
    pub repopulation_count: usize,
    pub best_energy: Option<f64>,
    pub mean_energy: Option<f64>,
    pub worst_energy: Option<f64>,
    pub origin_mix: Vec<GaOriginCount>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GaGenerationObservation {
    pub generation: usize,
    pub phase: String,
    pub request_count: usize,
    pub success_count: usize,
    pub failure_count: usize,
    pub failure_kind_counts: BTreeMap<String, usize>,
    pub converged_count: usize,
    pub population_size: usize,
    pub valid_population_size: usize,
    pub duplicate_count: usize,
    pub duplicate_hashkey_count: usize,
    pub duplicate_pmoi_count: usize,
    pub duplicate_energy_tol_count: usize,
    pub repopulated_count: usize,
    pub best_energy: Option<f64>,
    pub mean_energy: Option<f64>,
    pub worst_energy: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GaOriginCount {
    pub origin: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct GaPopulationMember {
    pub member_id: usize,
    pub label: String,
    pub origin: String,
    pub energy: Option<f64>,
    pub converged: bool,
    pub occurrences: usize,
    pub canonical_hashkey: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GaTopCandidate {
    pub unique_rank: usize,
    pub population_rank: Option<usize>,
    pub origin: String,
    pub occurrences: usize,
    pub energy: Option<f64>,
    pub converged: bool,
    pub label: String,
    pub structure_path: Option<String>,
    pub canonical_hashkey: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StructureDigest {
    pub label: String,
    pub formula: String,
    pub site_count: usize,
    pub dimensionality: String,
    pub energy: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StructurePreview {
    pub sites: Vec<StructurePreviewSite>,
    pub lattice: StructurePreviewLattice,
}

#[derive(Debug, Clone, Serialize)]
pub struct StructurePreviewSite {
    pub species: Vec<StructurePreviewSpecies>,
    pub abc: [f64; 3],
    pub xyz: [f64; 3],
    pub label: String,
    pub properties: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StructurePreviewSpecies {
    pub element: String,
    pub occu: f64,
    pub oxidation_state: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct StructurePreviewLattice {
    pub matrix: [[f64; 3]; 3],
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub alpha: f64,
    pub beta: f64,
    pub gamma: f64,
    pub volume: f64,
    pub pbc: [bool; 3],
}

#[derive(Debug, Clone, Serialize)]
pub struct ArtifactDigest {
    pub manifest_path: Option<String>,
    pub declared_artifacts: usize,
    pub latest_checkpoint: Option<String>,
    pub generation_state_latest: Option<String>,
    pub structure_exports: Option<String>,
    pub controller_trace: Option<String>,
    pub search_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CatalogSyncReport {
    pub node_stable_id: String,
    pub workspace_stable_id: String,
    pub synced_workspaces: usize,
    pub synced_runs: usize,
    pub synced_run_specs: usize,
    pub synced_structures: usize,
    pub synced_evaluations: usize,
    pub synced_artifacts: usize,
    pub emitted_events: usize,
    pub emitted_sync_receipts: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct DataFlowLab {
    pub table_counts: Vec<TableCount>,
    pub import_pipeline: Vec<FlowStep>,
    pub export_pipeline: Vec<FlowStep>,
    pub workflow_chains: Vec<WorkflowChain>,
    pub current_capabilities: Vec<String>,
    pub current_gaps: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TableCount {
    pub table: String,
    pub count: usize,
    pub role: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FlowStep {
    pub stage: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowChain {
    pub name: String,
    pub steps: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutoEmulateLab {
    pub analysis_root: String,
    pub discovered: usize,
    pub experiments: Vec<AutoEmulateExperiment>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutoEmulateExperiment {
    pub experiment_name: String,
    pub artifact_dir: String,
    pub campaign_id: String,
    pub branch_id: String,
    pub ga_run_name: Option<String>,
    pub feature_family: String,
    pub feature_version: String,
    pub feature_count: usize,
    pub training_observation_count: usize,
    pub pending_candidate_count: usize,
    pub selected_model_name: String,
    pub model_variant: String,
    pub fidelity: String,
    pub incumbent_target: f64,
    pub best_ranked_candidate_id: Option<String>,
    pub highest_uncertainty_candidate_id: Option<String>,
    pub top_acquisition_score: Option<f64>,
    pub top_uncertainty_score: Option<f64>,
    pub checkpoint_path: Option<String>,
    pub training_target_summary: Option<AutoEmulateScalarSummary>,
    pub predicted_target_summary: Option<AutoEmulateScalarSummary>,
    pub predicted_variance_summary: Option<AutoEmulateScalarSummary>,
    pub acquisition_score_summary: Option<AutoEmulateScalarSummary>,
    pub uncertainty_score_summary: Option<AutoEmulateScalarSummary>,
    pub top_ranked_candidates: Vec<AutoEmulateCandidateDiagnostic>,
    pub highest_uncertainty_candidates: Vec<AutoEmulateCandidateDiagnostic>,
    pub predictions: Vec<AutoEmulatePrediction>,
    pub feature_names: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoEmulateScalarSummary {
    pub min: f64,
    pub max: f64,
    pub mean: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoEmulateCandidateDiagnostic {
    pub candidate_id: String,
    pub label: Option<String>,
    pub rank: usize,
    pub predicted_mean: f64,
    pub predicted_variance: f64,
    pub acquisition_score: f64,
    pub uncertainty_score: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct AutoEmulatePrediction {
    pub candidate_id: String,
    pub label: Option<String>,
    pub rank: usize,
    pub predicted_mean: f64,
    pub predicted_variance: f64,
    pub acquisition_score: f64,
    pub uncertainty_score: f64,
    pub features: Vec<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MaterialFlowRequest {
    pub flow_id: String,
    pub cif_path: String,
    pub engine: String,
    pub tolerance: f64,
    pub normalization_target: Option<String>,
    pub perturbation_sigma: Option<f64>,
    pub perturbation_count: Option<usize>,
    pub perturbation_seed: Option<u64>,
    pub perturbation_max_displacement: Option<f64>,
    pub perturbation_min_distance: Option<f64>,
    pub perturbation_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MaterialFlowResponse {
    pub flow_id: String,
    pub title: String,
    pub source_path: String,
    pub engine: String,
    pub status: String,
    pub input_kind: String,
    pub output_kind: String,
    pub metrics: Vec<MaterialFlowMetric>,
    pub operations: Vec<MaterialFlowOperation>,
    pub artifacts: Vec<MaterialFlowArtifact>,
    pub input_preview: StructurePreview,
    pub output_preview: Option<StructurePreview>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MaterialFlowMetric {
    pub label: String,
    pub value: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MaterialFlowOperation {
    pub label: String,
    pub kind: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MaterialFlowArtifact {
    pub label: String,
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkspaceRunRequest {
    pub run_name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SurfaceLabRequest {
    pub cif_path: String,
    pub h: i32,
    pub k: i32,
    pub l: i32,
    pub thickness_angstrom: f64,
    pub vacuum_angstrom: f64,
    pub repeat_a: usize,
    pub repeat_b: usize,
    pub dedup_slab: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SurfaceLabResponse {
    pub source_path: String,
    pub parent_label: String,
    pub parent_preview: StructurePreview,
    pub slab_preview: StructurePreview,
    pub diagnostics: SurfaceLabDiagnostics,
}

#[derive(Debug, Clone, Serialize)]
pub struct SurfaceLabDiagnostics {
    pub slab_label: String,
    pub miller: [i32; 3],
    pub parent_atom_count: usize,
    pub slab_atom_count: usize,
    pub topology_safe_cut: Option<bool>,
    pub chosen_cut_offset_angstrom: Option<f64>,
    pub interplanar_spacing_angstrom: Option<f64>,
    pub broken_bond_estimate: Option<usize>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TopologyLabRequest {
    pub run_name: Option<String>,
    pub structure_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TopologyLabResponse {
    pub run_name: String,
    pub source_path: Option<String>,
    pub source_preview: StructurePreview,
    pub radius: f64,
    pub radius_mode: String,
    pub radius_const: f64,
    pub graph: TopologyGraphView,
    pub summary: TopologySummaryView,
    pub graph_text: String,
    pub canonical_hashkey: Option<String>,
    pub hashkey_error: Option<String>,
    pub near_cutoff_pairs: Vec<TopologyPairMarginView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TopologyGraphView {
    pub nodes: Vec<TopologyNodeView>,
    pub edges: Vec<[usize; 2]>,
    pub color_partitions: Vec<Vec<usize>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TopologyNodeView {
    pub index: usize,
    pub species: String,
    pub degree: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct TopologySummaryView {
    pub node_count: usize,
    pub edge_count: usize,
    pub connected_components: usize,
    pub degree_histogram_by_species: BTreeMap<String, BTreeMap<usize, usize>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TopologyPairMarginView {
    pub left: usize,
    pub right: usize,
    pub left_species: String,
    pub right_species: String,
    pub distance: f64,
    pub margin: f64,
    pub is_edge: bool,
}
