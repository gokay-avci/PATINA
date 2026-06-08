use crate::{EvaluationRecord, StructureRecord};
use serde::{Deserialize, Serialize};

/// Compact BH walker state for future resumable basin hopping workflows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BhWalkerState {
    /// Stable walker identifier within a search.
    pub walker_id: String,
    /// Restart or fresh-seed provenance for this walker.
    pub restart: WalkerRestartState,
    /// Current accepted structure/evaluation.
    pub current: EvaluationRecord,
    /// Best-so-far structure/evaluation.
    pub best: EvaluationRecord,
    /// Completed BH step count.
    pub step: usize,
    /// Most recent move-class activation seen for this walker, when available.
    pub move_class_activation: Option<MoveClassActivationRecord>,
    /// Whether a recovered restart artifact matches the current or best walker state.
    #[serde(default)]
    pub restart_equivalence: Option<WalkerRestartEquivalenceRecord>,
    /// Typed scientific controller state around BH move scheduling, when available.
    #[serde(default)]
    pub scientific_state: Option<BhWalkerScientificState>,
}

/// Source of a BH walker state when reconstructed or resumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WalkerStartSource {
    FreshSeed,
    RestartArtifact,
}

/// Compact provenance for how a BH walker entered the current run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WalkerRestartState {
    /// Whether the walker started from a fresh seed or from a recovered artifact.
    pub source: WalkerStartSource,
    /// Human-readable source label carried into the walker lifecycle.
    pub origin_label: String,
    /// Controller step represented by the restored or seeded state.
    pub restored_step: usize,
    /// Optional artifact path used to restore the walker state.
    pub source_output_path: Option<String>,
}

/// Source artifact kind used to recover or compare a BH walker restart state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WalkerRestartArtifactKind {
    WalkerCan,
    SavedOutput,
}

/// Compact restart-equivalence record comparing a restored walker artifact to live BH state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WalkerRestartEquivalenceRecord {
    /// Restart artifact kind used for this equivalence record.
    pub artifact_kind: WalkerRestartArtifactKind,
    /// Artifact path used to restore or compare the restart state, when available.
    pub artifact_path: Option<String>,
    /// Structure carried by the recovered restart artifact.
    pub restored_structure: StructureRecord,
    /// Whether the restored restart structure matches the current BH walker state.
    pub matches_current: bool,
    /// Whether the restored restart structure matches the best-so-far BH walker state.
    pub matches_best: bool,
}

/// Compact typed move-class label for BH provenance and artifacts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BhMoveClassRecord {
    MonteCarlo,
    SwapCations,
    SwapAtoms,
    MutateCluster,
    TwistCluster,
    TranslateCluster,
    RotateCluster,
}

/// Compact BH move-class activation snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MoveClassActivationRecord {
    /// Controller step where this move-class snapshot was observed.
    pub step: usize,
    /// Move-class used for that BH step.
    pub move_class: BhMoveClassRecord,
    /// Step size applied when this move-class snapshot was recorded.
    pub step_size: f64,
    /// Whether the resulting BH step was accepted.
    pub accepted: bool,
    /// Evaluated energy recorded for the moved candidate, when available.
    pub energy: Option<f64>,
}

/// Compact typed BH acceptance rule captured in scientific configuration or live state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BhAcceptanceRuleRecord {
    Metropolis { temperature: f64 },
    Quench,
    EnergyThreshold { threshold: f64 },
}

/// Compact typed BH method captured in scientific configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BhMethodRecord {
    Relax,
    Fixed,
    Oscillate {
        high_temperature_steps: usize,
        low_temperature_steps: usize,
    },
}

/// Compact typed BH scientific configuration around the move and acceptance kernels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BhScientificConfig {
    /// Maximum number of BH steps requested for this run.
    pub max_steps: usize,
    /// Number of walkers tracked by the controller.
    pub walkers: usize,
    /// Base BH step size requested for the move kernel.
    pub base_step_size: f64,
    /// Configured acceptance rule before any method-driven schedule changes.
    pub acceptance_rule: BhAcceptanceRuleRecord,
    /// Configured BH method or schedule family.
    pub method: BhMethodRecord,
    /// Rejection threshold that increases the active BH step size.
    pub dynamic_threshold: usize,
    /// Rejection threshold that unlocks richer move classes.
    pub moveclass_threshold: usize,
    /// Maximum multiplier allowed for dynamic BH step expansion.
    pub max_dynamic_step_multiplier: f64,
    /// Probability of selecting cation swapping once richer move classes are enabled.
    pub prob_switch_cations: f64,
    /// Probability of selecting atom swapping once richer move classes are enabled.
    pub prob_switch_atoms: f64,
    /// Probability of selecting cluster mutation once richer move classes are enabled.
    pub prob_mutate_cluster: f64,
    /// Probability of selecting cluster twisting once richer move classes are enabled.
    pub prob_twist_cluster: f64,
    /// Probability of selecting cluster translation once richer move classes are enabled.
    pub prob_translate_cluster: f64,
    /// Probability of selecting cluster rotation once richer move classes are enabled.
    pub prob_rotate_cluster: f64,
}

/// Compact typed BH move-regime state around the extracted step-control kernel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BhMoveRegimeRecord {
    /// Consecutive rejected BH updates retained by the controller.
    pub consecutive_rejections: usize,
    /// Whether richer move classes are currently enabled.
    pub random_moveclass_enabled: bool,
    /// Active step size currently applied by the move kernel.
    pub active_step_size: f64,
}

/// Compact typed BH scientific controller state attached to one walker snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BhWalkerScientificState {
    /// Controller step represented by this scientific snapshot.
    pub controller_step: usize,
    /// Acceptance rule active after any BH-method schedule updates.
    pub active_acceptance_rule: BhAcceptanceRuleRecord,
    /// Current move-regime state shared by the BH controller.
    pub move_regime: BhMoveRegimeRecord,
    /// Matched relaxation level used for topology-aware comparison, when known.
    #[serde(default)]
    pub last_matched_relaxation_level: Option<usize>,
}

/// Compact source-kind label for staged-production seed provenance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RestartSeedSourceKind {
    InlineCandidate,
    RestartArtifact,
    HybridGaSelection,
    HybridGaCrossover,
}

/// Compact provenance for how a staged-production seed entered the workflow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestartSeedProvenance {
    /// High-level source category for this production seed.
    pub source_kind: RestartSeedSourceKind,
    /// Human-readable source artifact name when one exists.
    pub source_name: Option<String>,
    /// Upstream label that produced or selected the seed.
    pub source_label: String,
}

/// Compact backend-status label for one staged-production relaxation attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelaxationBackendStatusRecord {
    Converged,
    ConvergedWithGradientWarning,
    RequiresMoreCycles,
    Crashed,
    InvalidEnergy,
    RejectedByProcedure,
}

/// Compact convergence label for one staged-production relaxation attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelaxationConvergenceRecord {
    Accepted,
    RetryableFailure,
    FinalFailure,
}

/// Compact provenance for one recorded staged-production relaxation attempt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelaxationStageRecord {
    pub stage: u8,
    pub attempt: usize,
    pub backend_status: RelaxationBackendStatusRecord,
    pub convergence: RelaxationConvergenceRecord,
    pub accepted_stage: bool,
    pub energy: Option<f64>,
    pub gnorm: Option<f64>,
    pub relaxed_label: Option<String>,
    pub primary_output_path: Option<String>,
}

/// Compact duplicate/topology reason label for staged-production comparisons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TopologyComparisonReasonRecord {
    Hashkey,
    Pmoi,
    EnergyTolerance,
}

/// Compact duplicate/topology action label for staged-production comparisons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TopologyComparisonActionRecord {
    MatchedBestArchiveInputHashkey,
    MatchedBlacklist,
    MatchedImportedLibrary,
    MatchedCurrentRunHistory,
    MatchedExistingBestSet,
}

/// Compact topology-comparison provenance attached to staged-production summaries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TopologyComparisonRecord {
    pub analysis_requested: bool,
    pub topology_skip: bool,
    pub input_hashkey: Option<String>,
    pub final_hashkey: Option<String>,
    pub reason: Option<TopologyComparisonReasonRecord>,
    pub action: Option<TopologyComparisonActionRecord>,
    pub matched_candidate_label: Option<String>,
    pub matched_hashkey: Option<String>,
    pub matched_rank: Option<usize>,
}

/// Compact best-set decision label for staged-production promotion and match outcomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BestSetDecisionKind {
    Inserted,
    MatchedExisting,
    Rejected,
}

/// Compact best-set decision provenance attached to staged-production summaries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BestSetDecisionRecord {
    pub kind: BestSetDecisionKind,
    pub rank: Option<usize>,
    pub matched_candidate_label: Option<String>,
    pub matched_hashkey: Option<String>,
}

/// Hybrid scientific mode describing how a child entered the hybrid production lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HybridCrossoverScientificMode {
    GaDownstreamSelection,
    EnforcedCrossover,
}

/// Scientific config that controls how the hybrid lane forwards material into production.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HybridCrossoverConfig {
    pub scientific_mode: HybridCrossoverScientificMode,
    pub attempt_count: usize,
}

impl Default for HybridCrossoverConfig {
    fn default() -> Self {
        Self {
            scientific_mode: HybridCrossoverScientificMode::GaDownstreamSelection,
            attempt_count: 12,
        }
    }
}

/// Compact hybrid child-origin label aligned with Scott GA provenance vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HybridChildOrigin {
    Seed,
    Crosso,
    Mutate,
    Mutcrs,
    RePopM,
    RePopR,
    Unknown,
}

/// Compact provenance for one hybrid child attempt forwarded toward production.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HybridCrossoverAttemptRecord {
    pub scientific_mode: HybridCrossoverScientificMode,
    pub ga_generation: Option<usize>,
    pub attempt_index: Option<usize>,
    pub child_origin: HybridChildOrigin,
    pub selected_for_production: bool,
    pub parent_left_label: Option<String>,
    pub parent_right_label: Option<String>,
}

/// Serializable lineage for one GA population member.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GaMemberLineageRecord {
    pub origin_label: String,
    pub generation: Option<usize>,
    pub step: Option<usize>,
    #[serde(default)]
    pub parent_labels: Vec<String>,
    pub attempt: usize,
}

/// Native topology identity materialized for one GA member.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GaMemberTopologyRecord {
    pub canonical_hashkey: Option<String>,
    pub source_hashkey: Option<String>,
    pub relaxed_hashkey: Option<String>,
}

/// Compact repopulation source label for GA generation provenance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PopulationRepopulationSource {
    EliteMutation,
    RandomStructure,
}

/// Compact provenance for one repopulated GA member captured at a generation boundary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PopulationRepopulationRecord {
    pub member_id: usize,
    pub source: PopulationRepopulationSource,
    pub source_candidate_label: String,
    pub evaluation_label: String,
    pub converged: bool,
    pub energy: Option<f64>,
}

/// Compact GA population member state for checkpoints and resumable orchestration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GaMemberState {
    pub member_id: usize,
    pub origin: String,
    pub occurrences: usize,
    pub source: StructureRecord,
    pub evaluation: EvaluationRecord,
    #[serde(default)]
    pub lineage: Option<GaMemberLineageRecord>,
    #[serde(default)]
    pub topology: GaMemberTopologyRecord,
}

/// Compact GA generation state for checkpoints and resumable orchestration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GaGenerationState {
    pub generation: usize,
    pub population: Vec<GaMemberState>,
    pub elites: Vec<GaMemberState>,
    #[serde(default)]
    pub repopulation: Vec<PopulationRepopulationRecord>,
}

/// Sampling-family workflows that currently share a Monte-Carlo-like lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SamplingWorkflowFamily {
    BasinHopping,
    SolidSolutions,
    ScanSurface,
    SimulatedAnnealing,
    EnergyLid,
}

/// Serializable schedule snapshot for Monte-Carlo-like workflow controllers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SamplingCheckpointSchedule {
    Quench,
    FixedTemperature {
        temperature: f64,
    },
    Annealing {
        temperature: f64,
        scale: f64,
        hold_steps: usize,
    },
    EnergyLid {
        threshold: f64,
        increment: f64,
        runners_per_level: usize,
    },
}

/// Compact rejection payload for sampling-family checkpoints and artifacts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SamplingRejectionRecord {
    pub candidate_label: String,
    pub attempt: usize,
    pub message: String,
}

/// Compact typed state for one Monte-Carlo-like walker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SamplingWalkerState {
    pub family: SamplingWorkflowFamily,
    pub schedule: SamplingCheckpointSchedule,
    pub step: usize,
    pub accepted_steps: usize,
    pub rejected_steps: usize,
    pub current: Option<EvaluationRecord>,
    pub best: Option<EvaluationRecord>,
    pub last_rejection: Option<SamplingRejectionRecord>,
}

/// Compact provenance for the accepted state that becomes the next branch source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HoldingPointRecord {
    pub label: String,
    pub energy: f64,
    pub recorded_step: usize,
    pub threshold: Option<f64>,
    pub temperature: Option<f64>,
}

/// Compact lineage for a runner or quench branch emitted from a holding point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunnerBranchRecord {
    pub runner_index: usize,
    pub origin_label: String,
    pub origin_energy: f64,
    pub final_label: Option<String>,
    pub final_energy: Option<f64>,
    pub accepted_steps: usize,
    pub rejected_steps: usize,
}

/// Compact typed state for one energy-lid window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnergyLidWindowState {
    pub lid_index: usize,
    pub threshold: f64,
    pub active_basin: String,
    pub holding_point: Option<HoldingPointRecord>,
    pub lid_walker: SamplingWalkerState,
    pub runner_branches: Vec<RunnerBranchRecord>,
    pub runner_walkers: Vec<SamplingWalkerState>,
}

/// Compact typed state for one simulated-annealing structure execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimulatedAnnealingStructureState {
    pub holding_point: Option<HoldingPointRecord>,
    pub anneal_walker: SamplingWalkerState,
    pub quench_branch: Option<RunnerBranchRecord>,
    pub quench_walker: Option<SamplingWalkerState>,
}

/// Duplicate provenance for solid-solution library screening.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SolidSolutionDuplicateSource {
    ImportedLibrary,
    CurrentRun,
}

/// Duplicate match recorded for one solid-solution step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SolidSolutionDuplicateRecord {
    pub hashkey: String,
    pub source: SolidSolutionDuplicateSource,
    pub occurrences: usize,
}

/// Step-level controller decision for a solid-solution workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SolidSolutionStepDecision {
    Accepted,
    RejectedAcceptance,
    RejectedGeometry,
    RejectedDuplicateImportedLibrary,
    RejectedDuplicateCurrentRun,
    EvaluationFailed,
    SkippedEvaluation,
}

/// Compact state for one accepted or rejected solid-solution step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SolidSolutionStepState {
    pub step: usize,
    pub decision: SolidSolutionStepDecision,
    pub source: StructureRecord,
    pub evaluation: Option<EvaluationRecord>,
    pub hashkey: Option<String>,
    pub duplicate: Option<SolidSolutionDuplicateRecord>,
}

/// Compact library and step snapshot for solid-solution workflows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SolidSolutionRunState {
    pub imported_hashkeys: Vec<SolidSolutionDuplicateRecord>,
    pub current_run_hashkeys: Vec<SolidSolutionDuplicateRecord>,
    pub steps: Vec<SolidSolutionStepState>,
}
