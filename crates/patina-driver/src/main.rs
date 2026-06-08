#![forbid(unsafe_code)]
#![recursion_limit = "256"]

/*!
What this binary implements: the command-routing shell for the Rust-owned PATINA workflows and the
preserved SCOTT parity surfaces.
Design basis: workflow logic lives in `application`, crate composition and CLI argument binding
stay here, and concrete local/runtime wiring is progressively extracted behind typed requests.
Current posture: this binary remains the integration boundary for the workspace, but it should own
as little scientific, reporting, or adapter logic as possible.
*/

mod application;

use anyhow::{anyhow, bail, Context, Result};
use camino::Utf8PathBuf;
use clap::{Parser, Subcommand, ValueEnum};
use indexmap::IndexMap;
use patina_evaluator::ScottBackendMode;
use patina_evaluator::{MasterTemplateLayout, ScottProcedureIntent};
use patina_external::{
    BackendEvaluator, JanusMode, JanusOptimizer, ScottBackend, ScottSandboxTemplate,
};
use patina_raspa::{
    DensityGridBinning, DensityGridNormalization, MoyoSymmetryAnalyzer, PeriodicFramework,
    SymmetryAnalyzer, SymmetryTolerance,
};
use patina_runtime::{run_local_procedure, ScottBackendRoutingPolicy};
use patina_search::{DuplicatePolicy, ScottParityGeneticAlgorithm};
use patina_surface::{
    DedupConfig, MillerIndex, SlabReductionConfig, SurfaceCutStrategy, SurfaceFace,
    SurfaceGenerationConfig, SurfaceReconstructionMode, SurfaceSupercellConfig,
    SurfaceTerminationBias,
};
use patina_types::Candidate;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, ExitStatus, Stdio};
use std::time::Duration;

use crate::application::ga_commands::handle_ga_mode_command;
use crate::application::workflow_cli::WorkflowArgs;
use crate::application::workflow_commands::handle_workflow_command;

const TOPOLOGY_NEAR_EDGE_THRESHOLD: u32 = 4;
const TOPOLOGY_NEAR_COORDINATION_THRESHOLD: u32 = 4;
const MAX_REPOPULATION_EVAL_PASSES: usize = 4;
const SUPPORTED_CANDIDATE_SNAPSHOT_EXTENSIONS: &[&str] =
    &["json", "xyz", "extxyz", "cif", "car", "arc", "can"];

pub(crate) use application::scott_legacy_audit::{RunJobOverrideRecord, RunJobPatchSummary};
pub(crate) use application::scott_topology_types::{
    AtomSpecRecord, StructureAtom, StructureSummary, TopologyEventRow, TopologyNearnessComparison,
    TopologyNearnessMetadata, TopologyNearnessReport, TopologyNearnessSources,
    TopologyNearnessSummary, TopologyPairRequest,
};

#[derive(Debug, Parser)]
#[command(name = "patina-driver")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print the current implementation status.
    Status,
    /// Discover shared workflow contracts, files, and launch routes.
    Workflow(WorkflowArgs),
    /// Native SCOTT-owned `klmc_scott` entry points with tracked export through the preserved legacy pathway.
    #[command(name = "legacy-scott", alias = "run-scott-search")]
    RunScottSearch(RunScottSearchArgs),
    /// Unified Rust-owned GA entrypoint exposing the shared controller and concrete evaluation adapters.
    #[command(name = "run-ga")]
    RunGa(RunGaArgs),
    /// Legacy alias for the persistent-daemon GA adapter.
    #[command(name = "run-rust-janus-search", hide = true)]
    RunRustJanusSearch(Box<RunRustJanusSearchArgs>),
    /// Legacy alias for the monolithic Scott GA adapter.
    #[command(name = "run-scott-staged-ga", hide = true)]
    RunScottStagedGa(Box<RunScottStagedGaArgs>),
    /// One-shot SCOTT evaluator sandbox.
    EvaluateScott(EvaluateScottArgs),
    /// One-shot evaluator call with an explicitly selected backend.
    EvaluateBackend(EvaluateBackendArgs),
    /// One-shot monolithic Scott runtime evaluation using the new evaluator/runtime core.
    EvaluateScottStaged(EvaluateScottStagedArgs),
    /// Production-style monolithic Scott runtime evaluation over multiple candidates.
    RunScottProduction(RunScottProductionArgs),
    /// Rust-owned hybrid workflow: monolithic GA followed by monolithic production with optional emulate-guided seed promotion.
    RunHybridGaProduction(Box<RunHybridGaProductionArgs>),
    /// Rust-owned basin-hopping workflow using a typed kernel and backend evaluation ports.
    RunBasinHopping(RunBasinHoppingArgs),
    /// Rust-owned energy-lid workflow starting from top structures in a tracked GA run.
    RunEnergyLid(RunEnergyLidArgs),
    /// Rust-owned scan-surface workflow following SCOTT ScanBox move/acceptance semantics.
    RunScanSurface(RunScanSurfaceArgs),
    /// Rust-owned solid-solution workflow following SCOTT library/mix/acceptance semantics where typed state supports it.
    RunSolidSolutions(RunSolidSolutionsArgs),
    /// Score pending GA candidates with the isolated patina-emulate surrogate runtime.
    ScoreGaEmulate(ScoreGaEmulateArgs),
    /// Rust-owned simulated annealing workflow starting from top structures in a tracked GA run.
    RunSimulatedAnnealing(RunSimulatedAnnealingArgs),
    /// Rust-owned cluster perturbation workflow exposing perturbation, fingerprint, and duplicate ports.
    PerturbCluster(PerturbClusterArgs),
    /// Rust-owned framework-to-slab workflow for periodic structures.
    GenerateSurface(GenerateSurfaceArgs),
    /// Rust-owned surface-ion reconstruction workflow derived from surface generation plus Monte Carlo sampling.
    RunSurfaceReconstruction(RunSurfaceReconstructionArgs),
    /// Rust-owned polarity analysis for an existing 2D-periodic slab.
    AnalyzeSurfacePolarity(AnalyzeSurfacePolarityArgs),
    /// Rust-owned framework symmetry analysis and standardization for periodic structures.
    AnalyzeFrameworkSymmetry(AnalyzeFrameworkSymmetryArgs),
    /// Rust-owned framework normalization entrypoint for standardized or primitive-standardized cells.
    NormalizeFramework(NormalizeFrameworkArgs),
    /// Rust-owned framework GCMC workflow using periodic Janus/MACE evaluation and RASPA-style artifacts.
    RunFrameworkGcmc(RunFrameworkGcmcArgs),
    /// Focused structure utilities with Figment-backed input resolution.
    Structure(StructureArgs),
    /// Rust-owned stk-inspired 0D supramolecular construction and artifact export.
    #[command(name = "construct-stk-zero-d")]
    ConstructStkZeroD(ConstructStkZeroDArgs),
    /// Worker-pooled evaluator batch using transient or persistent runner mode.
    EvaluateBackendBatch(EvaluateBackendBatchArgs),
    /// Worker benchmark comparing transient and persistent batch evaluation.
    CompareBackendBatch(CompareBackendBatchArgs),
    /// Worker benchmark comparing transient and persistent evaluation across sequential generations.
    CompareBackendGenerations(CompareBackendGenerationsArgs),
    /// Tracked Rust-owned evaluator campaign with per-generation exports and topology summaries.
    RunBackendCampaign(RunBackendCampaignArgs),
    /// Compare Rust duplicate classification against duplicate events from a native tracked run.
    CompareDuplicateParity(CompareDuplicateParityArgs),
    /// Aggressive synthetic edge-case audit for hashkeys and duplicate reasons.
    CompareDuplicateEdgeCases(CompareDuplicateEdgeCasesArgs),
}

/// Unified entrypoint for Rust-owned GA execution.
///
/// Hexagonal structure:
/// - shared domain/controller: `ScottParityGeneticAlgorithm`
/// - shared application workflow: `GaWorkflowService`
/// - switched port implementation: one `GaEvaluationPort` adapter per subcommand
///
/// Use this command when the GA science should stay the same but the evaluation adapter,
/// runtime pathway, or backend connection semantics need to change explicitly.
#[derive(Debug, Parser)]
struct RunGaArgs {
    #[command(subcommand)]
    mode: GaModeCommand,
}

#[derive(Debug, Subcommand)]
enum GaModeCommand {
    /// Shared Rust GA controller with persistent daemon workers backed by Janus/MACE.
    ///
    /// Architecture:
    /// - controller/workflow: shared Rust GA core
    /// - evaluation port: `PersistentPoolGaEvaluator`
    /// - backend connection: `PersistentWorkerPool` + `PersistentJanusMaceBackend`
    ///
    /// Operational semantics:
    /// - long-lived Janus workers
    /// - stable per-worker sandboxes
    /// - parallelism controlled by `--workers`
    #[command(name = "persistent-daemon", alias = "janus-persistent")]
    PersistentDaemon(Box<RunRustJanusSearchArgs>),
    /// Shared Rust GA controller with the monolithic Scott runtime adapter.
    ///
    /// Architecture:
    /// - controller/workflow: shared Rust GA core
    /// - evaluation port: `StagedScottGaEvaluator`
    /// - runtime adapter: `run_local_procedure` + `RoutedBackendStageExecutor`
    ///
    /// Operational semantics:
    /// - Scott-like staged procedure execution
    /// - backend calls routed per stage
    /// - generation fan-out controlled by `RAYON_NUM_THREADS`
    #[command(name = "scott-monolithic", alias = "scott-staged")]
    ScottMonolithic(Box<RunScottStagedGaArgs>),
    /// Print the GA architecture map and the current adapter semantics.
    Explain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
enum SearchMode {
    Bh,
    Ga,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
enum LegacyScottJobTypeCli {
    ProductionRun,
    BasinHopping,
    GeneticAlgorithm,
    SolidSolutions,
    RefineSprings,
    ScanBox,
    SimulatedAnnealing,
    EnergyLid,
    Testing,
    HybridGaProduction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
enum EvalBackendKind {
    Scott,
    Gulp,
    JanusMace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum DriverStkZeroDTopology {
    LinearBridge,
    Trigonal,
    SquarePlanar,
    TrigonalCage,
    M2l4Lantern,
    TriangleMacrocycle,
    AlternatingMacrocycle,
    CappedPolymer,
    HostGuest,
    Rotaxane,
}

impl From<DriverStkZeroDTopology> for application::stk_construction::StkZeroDTopologyPreset {
    fn from(value: DriverStkZeroDTopology) -> Self {
        match value {
            DriverStkZeroDTopology::LinearBridge => Self::LinearBridge,
            DriverStkZeroDTopology::Trigonal => Self::Trigonal,
            DriverStkZeroDTopology::SquarePlanar => Self::SquarePlanar,
            DriverStkZeroDTopology::TrigonalCage => Self::TrigonalCage,
            DriverStkZeroDTopology::M2l4Lantern => Self::M2l4Lantern,
            DriverStkZeroDTopology::TriangleMacrocycle => Self::TriangleMacrocycle,
            DriverStkZeroDTopology::AlternatingMacrocycle => Self::AlternatingMacrocycle,
            DriverStkZeroDTopology::CappedPolymer => Self::CappedPolymer,
            DriverStkZeroDTopology::HostGuest => Self::HostGuest,
            DriverStkZeroDTopology::Rotaxane => Self::Rotaxane,
        }
    }
}

impl EvalBackendKind {
    fn engine_label(self) -> &'static str {
        match self {
            Self::Scott => "SCOTT+GULP",
            Self::Gulp => "GULP",
            Self::JanusMace => "Janus/MACE",
        }
    }

    fn supports_persistent_parallel_workers(self) -> bool {
        matches!(self, Self::JanusMace)
    }
}

impl SearchMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Bh => "bh",
            Self::Ga => "ga",
        }
    }

    fn into_legacy_job_type(self) -> application::legacy_scott::LegacyScottJobType {
        match self {
            Self::Bh => application::legacy_scott::LegacyScottJobType::BasinHopping,
            Self::Ga => application::legacy_scott::LegacyScottJobType::GeneticAlgorithm,
        }
    }

    fn from_legacy_job_type(
        job_type: application::legacy_scott::LegacyScottJobType,
    ) -> Option<Self> {
        match job_type {
            application::legacy_scott::LegacyScottJobType::BasinHopping => Some(Self::Bh),
            application::legacy_scott::LegacyScottJobType::GeneticAlgorithm => Some(Self::Ga),
            _ => None,
        }
    }
}

impl From<LegacyScottJobTypeCli> for application::legacy_scott::LegacyScottJobType {
    fn from(value: LegacyScottJobTypeCli) -> Self {
        match value {
            LegacyScottJobTypeCli::ProductionRun => Self::ProductionRun,
            LegacyScottJobTypeCli::BasinHopping => Self::BasinHopping,
            LegacyScottJobTypeCli::GeneticAlgorithm => Self::GeneticAlgorithm,
            LegacyScottJobTypeCli::SolidSolutions => Self::SolidSolutions,
            LegacyScottJobTypeCli::RefineSprings => Self::RefineSprings,
            LegacyScottJobTypeCli::ScanBox => Self::ScanBox,
            LegacyScottJobTypeCli::SimulatedAnnealing => Self::SimulatedAnnealing,
            LegacyScottJobTypeCli::EnergyLid => Self::EnergyLid,
            LegacyScottJobTypeCli::Testing => Self::Testing,
            LegacyScottJobTypeCli::HybridGaProduction => Self::HybridGaProduction,
        }
    }
}

impl From<DriverFrameworkNormalizationTarget> for application::ports::FrameworkNormalizationTarget {
    fn from(value: DriverFrameworkNormalizationTarget) -> Self {
        match value {
            DriverFrameworkNormalizationTarget::Standardized => Self::Standardized,
            DriverFrameworkNormalizationTarget::PrimitiveStandardized => {
                Self::PrimitiveStandardized
            }
        }
    }
}

#[derive(Debug, Parser)]
struct RunScottSearchArgs {
    /// Optional TOML configuration file for SCOTT search execution.
    #[arg(long)]
    config: Option<PathBuf>,
    /// Native SCOTT entry point executed internally by `klmc_scott`.
    #[arg(long, value_enum)]
    job_type: Option<LegacyScottJobTypeCli>,
    /// Legacy shorthand for the native SCOTT GA/BH entry points.
    #[arg(long, value_enum)]
    mode: Option<SearchMode>,
    /// Path to the direct libgulp-linked `klmc_scott` executable.
    #[arg(long)]
    scott_bin: Option<PathBuf>,
    /// Directory containing SCOTT runtime inputs such as `run.job`, `jobs`, `atoms.in`, and `Master.gin`.
    #[arg(long)]
    data_dir: Option<PathBuf>,
    /// Sandbox directory used for the SCOTT run.
    #[arg(long)]
    workdir: Option<PathBuf>,
    /// Tracked run directory under `runs/active/` or `runs/archive/`.
    #[arg(long)]
    run_dir: Option<PathBuf>,
    /// Human-readable system label stored in the run manifest.
    #[arg(long)]
    system: Option<String>,
    /// Search steps for BH.
    #[arg(long)]
    bh_steps: Option<usize>,
    /// Search generations for GA.
    #[arg(long)]
    ga_generations: Option<usize>,
    /// Population size or walker count.
    #[arg(long)]
    population: Option<usize>,
    /// Native SCOTT random seed. Use a positive integer for reproducible runs.
    #[arg(long)]
    seed: Option<u64>,
    /// Native SCOTT evaluator backend.
    #[arg(long)]
    evaluator_backend: Option<String>,
    /// Python executable used for the native SCOTT Janus adapter path.
    #[arg(long)]
    janus_python: Option<PathBuf>,
    /// Python adapter script used for the native SCOTT Janus path.
    #[arg(long)]
    janus_adapter: Option<PathBuf>,
    /// Janus evaluator mode used by native SCOTT.
    #[arg(long)]
    janus_mode: Option<String>,
    /// Janus architecture identifier.
    #[arg(long)]
    janus_arch: Option<String>,
    /// Janus model card or local model path.
    #[arg(long)]
    janus_model: Option<String>,
    /// Janus device such as `cpu`, `cuda`, or `mps`.
    #[arg(long)]
    janus_device: Option<String>,
    /// Janus dtype.
    #[arg(long)]
    janus_dtype: Option<String>,
    /// Janus local optimization force threshold.
    #[arg(long)]
    janus_fmax: Option<f64>,
    /// Janus local optimization step limit.
    #[arg(long)]
    janus_steps: Option<usize>,
    /// Search temperature.
    #[arg(long)]
    temperature: Option<f64>,
    /// Basin-hopping step size.
    #[arg(long)]
    step_size: Option<f64>,
    /// Initial cluster boundary.
    #[arg(long)]
    boundary: Option<f64>,
    /// Minimum first-neighbour distance.
    #[arg(long)]
    collapse: Option<f64>,
    /// Fragmentation threshold.
    #[arg(long)]
    fragment: Option<f64>,
    /// Second-neighbour distance control.
    #[arg(long)]
    dspecies: Option<f64>,
    /// Native SCOTT output verbosity.
    #[arg(long)]
    output_level: Option<usize>,
    /// Enable native SCOTT topological hashkey analysis.
    #[arg(long)]
    use_top_analysis: Option<bool>,
    /// Enable Dreadnaut hashkey generation when native topological analysis is requested.
    #[arg(long)]
    use_dreadnaut_keys: Option<bool>,
    /// Hashkey radius mode such as `IR` or `CR`.
    #[arg(long)]
    hashkey_radius: Option<String>,
    /// Additive constant used in the hashkey radius criterion.
    #[arg(long)]
    hashkey_radius_const: Option<f64>,
    /// Optional timeout in seconds.
    #[arg(long)]
    timeout_secs: Option<u64>,
}

#[derive(Debug, Parser)]
struct CompareDuplicateParityArgs {
    /// Tracked native run directory under `runs/active` or `runs/archive`.
    #[arg(long)]
    run_dir: PathBuf,
    /// PMOI duplicate threshold used by the Rust classifier during parity replay.
    #[arg(long, default_value_t = 0.02)]
    pmoi_tolerance: f64,
}

#[derive(Debug, Parser)]
struct CompareDuplicateEdgeCasesArgs {
    /// Output directory for the synthetic duplicate edge-case report.
    #[arg(long, default_value = "scratch/duplicate_edge_cases")]
    output_dir: PathBuf,
    /// Optional atoms table override used for native-compatible hashkey radius and species ordering.
    #[arg(long)]
    atoms_in_template: Option<PathBuf>,
    /// Enable exact Dreadnaut hashkeys for duplicate classification.
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    use_dreadnaut_keys: bool,
    /// Hashkey radius mode, matching native SCOTT conventions.
    #[arg(long, default_value = "IR")]
    hashkey_radius: String,
    /// Extra hashkey radius constant, matching native SCOTT conventions.
    #[arg(long, default_value_t = 0.4)]
    hashkey_radius_const: f64,
    /// PMOI duplicate threshold used by the Rust classifier for edge-case probing.
    #[arg(long, default_value_t = 0.02)]
    pmoi_tolerance: f64,
}

#[derive(Debug, Parser)]
struct RunRustJanusSearchArgs {
    /// Figment configuration file (`.toml` or `.json`) using stage/input/ga sections.
    #[arg(long)]
    config: Option<PathBuf>,
    /// Stage directory used when no explicit base candidate path is provided. Defaults to the current directory.
    #[arg(long)]
    stage_dir: Option<PathBuf>,
    /// Base candidate JSON used as the legacy typed seed structure for the Rust-owned search.
    #[arg(long)]
    base_candidate_json: Option<PathBuf>,
    /// Cluster GA seed structure. SCOTT-parity stage inference accepts exactly one `.xyz` file.
    #[arg(long)]
    base_candidate_path: Option<PathBuf>,
    /// Sandbox directory used for the Rust-owned Janus run.
    #[arg(long)]
    workdir: PathBuf,
    /// Optional Rust GA checkpoint to resume from.
    #[arg(long)]
    resume_from_checkpoint: Option<PathBuf>,
    /// Tracked run directory under `runs/active/` or `runs/archive/`.
    #[arg(long)]
    run_dir: PathBuf,
    /// Human-readable system label stored in the run manifest.
    #[arg(long, default_value = "unknown-system")]
    system: String,
    /// Number of GA generations.
    #[arg(long, default_value_t = 10)]
    ga_generations: usize,
    /// Population size.
    #[arg(long, default_value_t = 10)]
    population: usize,
    /// Number of persistent daemon worker slots.
    #[arg(long)]
    workers: Option<usize>,
    /// Keep worker directories after evaluation for debugging.
    #[arg(long, default_value_t = false)]
    keep_dirs: bool,
    /// Optional timeout in seconds for each Janus request.
    #[arg(long)]
    timeout_secs: Option<u64>,
    /// Optional random seed for deterministic controller behavior.
    #[arg(long)]
    seed: Option<u64>,
    /// Search temperature reserved for later acceptance/replacement parity work.
    #[arg(long, default_value_t = 10.0)]
    temperature: f64,
    /// Geometric perturbation size used by the Rust controller.
    #[arg(long, default_value_t = 0.1)]
    step_size: f64,
    /// Duplicate-policy mode for the Rust-owned lane. Native-compatible external hashkey is the only scientific mode.
    #[arg(long, default_value = "external-native-hashkey")]
    duplicate_policy_mode: RustJanusDuplicatePolicyCliMode,
    /// Python executable used for the Janus adapter.
    #[arg(long)]
    python_bin: Option<PathBuf>,
    /// Python adapter script used for the Janus/MACE backend.
    #[arg(long)]
    janus_adapter_script: Option<PathBuf>,
    /// Janus architecture, for example `mace_mp`.
    #[arg(long, default_value = "mace_mp")]
    janus_arch: String,
    /// Janus model card or local model path.
    #[arg(long, default_value = "small")]
    janus_model: String,
    /// Janus device such as `cpu`, `cuda`, or `mps`.
    #[arg(long, default_value = "cpu")]
    janus_device: String,
    /// Janus calculator dtype.
    #[arg(long, default_value = "float64")]
    janus_dtype: String,
    /// Janus evaluation mode.
    #[arg(long, default_value = "local-opt")]
    janus_mode: DriverJanusMode,
    /// Janus local optimization force threshold.
    #[arg(long, default_value_t = 0.1)]
    janus_fmax: f64,
    /// Janus local optimization step limit.
    #[arg(long, default_value_t = 1000)]
    janus_steps: usize,
    /// Janus local optimization algorithm.
    #[arg(long, default_value = "abc-fire")]
    janus_optimizer: DriverJanusOptimizer,
    /// Optional atoms table override used for native-compatible hashkey radius and species ordering.
    #[arg(long)]
    atoms_in_template: Option<PathBuf>,
    /// Enable exact Dreadnaut hashkeys for duplicate classification.
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    use_dreadnaut_keys: bool,
    /// Hashkey radius mode, matching native SCOTT conventions.
    #[arg(long, default_value = "IR")]
    hashkey_radius: String,
    /// Extra hashkey radius constant, matching native SCOTT conventions.
    #[arg(long, default_value_t = 0.4)]
    hashkey_radius_const: f64,
    /// PMOI duplicate threshold. Lower is stricter, higher is more permissive.
    #[arg(long, default_value_t = 0.02)]
    pmoi_tolerance: f64,
    /// Enable PMOI as a duplicate fallback after exact native hashkey comparison.
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    enable_pmoi: bool,
    /// Optional controller-side GA replacement ratio.
    #[arg(long)]
    pop_replacement_ratio: Option<f64>,
    /// Optional controller-side elite reinsertion ratio.
    #[arg(long)]
    reinsert_elites_ratio: Option<f64>,
    /// Optional controller-side mutation ratio.
    #[arg(long)]
    mutation_ratio: Option<f64>,
    /// Optional controller-side self-crossover fraction within mutation events.
    #[arg(long)]
    mut_selfcross_ratio: Option<f64>,
    /// Optional controller-side repopulation attempt cap.
    #[arg(long)]
    max_repop_attempts: Option<usize>,
    /// Optional controller-side crossover attempt count.
    #[arg(long)]
    crossover_attempts: Option<usize>,
    /// Optional controller-side minimum tournament size.
    #[arg(long)]
    tournament_size_min: Option<usize>,
    /// Optional controller-side maximum tournament size.
    #[arg(long)]
    tournament_size_max: Option<usize>,
    /// Internal operator-policy backend selector used when this config is built from other entry points.
    #[arg(skip = None)]
    operator_policy_backend: Option<String>,
}

#[derive(Debug, Parser)]
struct ScoreGaEmulateArgs {
    /// Tracked GA run directory containing `raw/generation_XXXX_state.json` snapshots.
    #[arg(long)]
    ga_run_dir: PathBuf,
    /// Output root for surrogate request/response and scoring artifacts.
    #[arg(long)]
    output_dir: PathBuf,
    /// Pending candidate snapshots to score. Accepts `.json`, `.xyz`, `.extxyz`, `.cif`, `.car`, `.arc`, or `.can`.
    #[arg(long)]
    pending_candidate: Vec<PathBuf>,
    /// Optional directory of pending candidate snapshots to score.
    #[arg(long)]
    pending_candidate_dir: Option<PathBuf>,
    /// Stable emulate campaign identifier. Defaults to a name derived from the GA run directory.
    #[arg(long)]
    campaign_id: Option<String>,
    /// Stable emulate branch identifier.
    #[arg(long, default_value = "branch-ga-1")]
    branch_id: String,
    /// Learning objective requested from the surrogate layer.
    #[arg(long, value_enum, default_value = "reduce-uncertainty")]
    objective: EmulateLearningObjectiveCli,
    /// Fidelity label attached to the training and pending rows.
    #[arg(long, value_enum, default_value = "janus-mace-low")]
    fidelity: EmulateFidelityCli,
    /// Feature projector family used to build emulator input rows.
    #[arg(long, value_enum, default_value = "pair-distance-signature")]
    feature_projector: EmulateFeatureProjectorCli,
    /// Target name emitted into the surrogate contract.
    #[arg(long, default_value = "energy")]
    target_name: String,
    /// Optional target unit emitted into the surrogate contract.
    #[arg(long, default_value = "eV")]
    target_unit: String,
    /// Surrogate model family.
    #[arg(long, value_enum, default_value = "whitened-svgp")]
    model_variant: SurrogateModelVariantCli,
    /// Surrogate optimization direction.
    #[arg(long, value_enum, default_value = "minimize")]
    surrogate_objective: SurrogateObjectiveCli,
    /// Number of training epochs.
    #[arg(long, default_value_t = 100)]
    epochs: usize,
    /// Number of inducing points.
    #[arg(long, default_value_t = 24)]
    num_inducing: usize,
    /// Mini-batch size.
    #[arg(long, default_value_t = 32)]
    batch_size: usize,
    /// Learning rate.
    #[arg(long, default_value_t = 0.05)]
    lr: f64,
    /// Bootstrap ensemble count.
    #[arg(long, default_value_t = 1)]
    n_bootstraps: usize,
    /// Random seed for the surrogate runtime.
    #[arg(long, default_value_t = 42)]
    random_seed: u64,
    /// Surrogate runtime log level.
    #[arg(long, default_value = "warning")]
    log_level: String,
    /// Optional uv binary path.
    #[arg(long)]
    uv_bin: Option<PathBuf>,
    /// Optional override for the isolated patina-emulate Python project directory.
    #[arg(long)]
    emulate_project_dir: Option<PathBuf>,
    /// uv environment name under the workspace `venvs/` directory.
    #[arg(long, default_value = "autoemulate")]
    emulate_environment: String,
    /// Optional runtime timeout in seconds.
    #[arg(long, default_value_t = 300)]
    timeout_secs: u64,
}

#[derive(Debug, Parser)]
struct RunScottStagedGaArgs {
    /// Figment configuration file (`.toml` or `.json`) using stage/input/ga sections.
    #[arg(long)]
    config: Option<PathBuf>,
    /// Stage directory used when no explicit base candidate path is provided. Defaults to the current directory.
    #[arg(long)]
    stage_dir: Option<PathBuf>,
    /// Base candidate JSON used as the legacy typed seed structure for the Rust-owned GA controller.
    #[arg(long)]
    base_candidate_json: Option<PathBuf>,
    /// Cluster GA seed structure. SCOTT-parity stage inference accepts exactly one `.xyz` file.
    #[arg(long)]
    base_candidate_path: Option<PathBuf>,
    /// Sandbox directory used for the monolithic Scott GA run.
    #[arg(long)]
    workdir: PathBuf,
    /// Directory containing monolithic runtime inputs such as `run.job`, `Master.gin`, and optional sidecars.
    #[arg(long)]
    scott_input_dir: Option<PathBuf>,
    /// Optional Rust GA checkpoint to resume from.
    #[arg(long)]
    resume_from_checkpoint: Option<PathBuf>,
    /// Tracked run directory used for monolithic GA outputs.
    #[arg(long)]
    run_dir: PathBuf,
    /// Human-readable system label stored in the run manifest.
    #[arg(long, default_value = "unknown-system")]
    system: String,
    /// Number of GA generations.
    #[arg(long, default_value_t = 10)]
    ga_generations: usize,
    /// Population size.
    #[arg(long, default_value_t = 10)]
    population: usize,
    /// Keep staged request directories after evaluation for debugging.
    #[arg(long, default_value_t = false)]
    keep_dirs: bool,
    /// Optional timeout in seconds for each backend request.
    #[arg(long)]
    timeout_secs: Option<u64>,
    /// Optional random seed for deterministic controller behavior.
    #[arg(long)]
    seed: Option<u64>,
    /// Search temperature reserved for later acceptance/replacement parity work.
    #[arg(long, default_value_t = 10.0)]
    temperature: f64,
    /// Geometric perturbation size used by the Rust controller.
    #[arg(long, default_value_t = 0.1)]
    step_size: f64,
    /// Python executable used for the Janus adapter when routing includes `janus_mace`.
    #[arg(long)]
    python_bin: Option<PathBuf>,
    /// Python adapter script used for the Janus/MACE backend when routing includes `janus_mace`.
    #[arg(long)]
    janus_adapter_script: Option<PathBuf>,
    /// Janus architecture, for example `mace_mp`.
    #[arg(long, default_value = "mace_mp")]
    janus_arch: String,
    /// Janus model card or local model path.
    #[arg(long, default_value = "small")]
    janus_model: String,
    /// Janus device such as `cpu`, `cuda`, or `mps`.
    #[arg(long, default_value = "cpu")]
    janus_device: String,
    /// Janus calculator dtype.
    #[arg(long, default_value = "float64")]
    janus_dtype: String,
    /// Janus evaluation mode used when routing includes `janus_mace`.
    #[arg(long, default_value = "local-opt")]
    janus_mode: DriverJanusMode,
    /// Janus local optimization force threshold.
    #[arg(long, default_value_t = 0.1)]
    janus_fmax: f64,
    /// Janus local optimization step limit.
    #[arg(long, default_value_t = 1000)]
    janus_steps: usize,
    /// Janus local optimization algorithm.
    #[arg(long, default_value = "abc-fire")]
    janus_optimizer: DriverJanusOptimizer,
    /// Path to the GULP executable when routing includes `gulp`.
    #[arg(long)]
    executable: Option<PathBuf>,
    /// Path to the Master.gin template used by the monolithic Scott evaluator.
    #[arg(long)]
    master_gin_template: Option<PathBuf>,
    /// Path to the SCOTT run.job template used to build the monolithic evaluator procedure.
    #[arg(long)]
    run_job_template: Option<PathBuf>,
    /// Default backend for the monolithic runtime.
    #[arg(long)]
    runtime_default_backend: Option<String>,
    /// Per-stage backend override in the form `<stage>=<backend>`.
    #[arg(long = "runtime-stage-backend")]
    runtime_stage_backend: Vec<String>,
    /// Optional atoms table used to override built-in species radii/order for exact dreadnaut hashkeys.
    #[arg(long)]
    atoms_in_template: Option<PathBuf>,
    /// Enable exact Dreadnaut hashkeys for duplicate classification.
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    use_dreadnaut_keys: bool,
    /// Hashkey radius mode, matching native SCOTT conventions.
    #[arg(long, default_value = "IR")]
    hashkey_radius: String,
    /// Extra hashkey radius constant, matching native SCOTT conventions.
    #[arg(long, default_value_t = 0.4)]
    hashkey_radius_const: f64,
    /// PMOI duplicate threshold. Lower is stricter, higher is more permissive.
    #[arg(long, default_value_t = 0.02)]
    pmoi_tolerance: f64,
    /// Enable PMOI as a duplicate fallback after exact native hashkey comparison.
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    enable_pmoi: bool,
    /// Optional controller-side GA replacement ratio.
    #[arg(long)]
    pop_replacement_ratio: Option<f64>,
    /// Optional controller-side elite reinsertion ratio.
    #[arg(long)]
    reinsert_elites_ratio: Option<f64>,
    /// Optional controller-side mutation ratio.
    #[arg(long)]
    mutation_ratio: Option<f64>,
    /// Optional controller-side self-crossover fraction within mutation events.
    #[arg(long)]
    mut_selfcross_ratio: Option<f64>,
    /// Optional controller-side repopulation attempt cap.
    #[arg(long)]
    max_repop_attempts: Option<usize>,
    /// Optional controller-side crossover attempt count.
    #[arg(long)]
    crossover_attempts: Option<usize>,
    /// Optional controller-side minimum tournament size.
    #[arg(long)]
    tournament_size_min: Option<usize>,
    /// Optional controller-side maximum tournament size.
    #[arg(long)]
    tournament_size_max: Option<usize>,
    /// Enable the research-lane uncertainty gate so low-uncertainty offspring can use emulated energies.
    #[arg(long, default_value_t = false)]
    emulate_uncertainty_gate: bool,
    /// Stable emulate campaign identifier used when `--emulate-uncertainty-gate` is enabled.
    #[arg(long)]
    emulate_campaign_id: Option<String>,
    /// Stable emulate branch identifier used when `--emulate-uncertainty-gate` is enabled.
    #[arg(long, default_value = "branch-ga-1")]
    emulate_branch_id: String,
    /// Learning objective requested from the surrogate layer when the uncertainty gate is enabled.
    #[arg(long, value_enum, default_value = "reduce-uncertainty")]
    emulate_objective: EmulateLearningObjectiveCli,
    /// Fidelity label attached to exact observations recorded for the uncertainty gate.
    #[arg(long, value_enum, default_value = "gulp-reference")]
    emulate_fidelity: EmulateFidelityCli,
    /// Feature projector family used to build emulator input rows.
    #[arg(long, value_enum, default_value = "pair-distance-signature")]
    emulate_feature_projector: EmulateFeatureProjectorCli,
    /// Target name emitted into the surrogate contract.
    #[arg(long, default_value = "energy")]
    emulate_target_name: String,
    /// Optional target unit emitted into the surrogate contract.
    #[arg(long, default_value = "eV")]
    emulate_target_unit: String,
    /// Surrogate model family.
    #[arg(long, value_enum, default_value = "whitened-svgp")]
    emulate_model_variant: SurrogateModelVariantCli,
    /// Surrogate optimization direction.
    #[arg(long, value_enum, default_value = "minimize")]
    emulate_surrogate_objective: SurrogateObjectiveCli,
    /// Number of training epochs.
    #[arg(long, default_value_t = 120)]
    emulate_epochs: usize,
    /// Number of inducing points.
    #[arg(long, default_value_t = 24)]
    emulate_num_inducing: usize,
    /// Mini-batch size.
    #[arg(long, default_value_t = 16)]
    emulate_batch_size: usize,
    /// Learning rate.
    #[arg(long, default_value_t = 0.05)]
    emulate_lr: f64,
    /// Bootstrap ensemble count.
    #[arg(long, default_value_t = 1)]
    emulate_n_bootstraps: usize,
    /// Random seed for the surrogate runtime.
    #[arg(long, default_value_t = 42)]
    emulate_random_seed: u64,
    /// Surrogate runtime log level.
    #[arg(long, default_value = "warning")]
    emulate_log_level: String,
    /// Optional uv binary path.
    #[arg(long)]
    emulate_uv_bin: Option<PathBuf>,
    /// Optional override for the isolated patina-emulate Python project directory.
    #[arg(long)]
    emulate_project_dir: Option<PathBuf>,
    /// uv environment name under the workspace `venvs/` directory.
    #[arg(long, default_value = "autoemulate")]
    emulate_environment: String,
    /// Optional surrogate runtime timeout in seconds.
    #[arg(long, default_value_t = 300)]
    emulate_timeout_secs: u64,
    /// Number of fully exact generations used to warm-start the emulate gate.
    #[arg(long, default_value_t = 1)]
    emulate_warmup_generations: usize,
    /// Minimum number of exact observations required before the emulate gate is allowed to score a batch.
    #[arg(long, default_value_t = 24)]
    emulate_min_training_observations: usize,
    /// Requests with uncertainty scores above this threshold are sent to the exact evaluator.
    #[arg(long, default_value_t = 0.5)]
    emulate_uncertainty_threshold: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ScottSearchConfigFile {
    job_type: Option<application::legacy_scott::LegacyScottJobType>,
    mode: Option<SearchMode>,
    scott_bin: Option<PathBuf>,
    data_dir: Option<PathBuf>,
    workdir: Option<PathBuf>,
    run_dir: Option<PathBuf>,
    system: Option<String>,
    timeout_secs: Option<u64>,
    #[serde(default)]
    shared: ScottSharedSettings,
    #[serde(default)]
    backend: ScottBackendSettings,
    #[serde(default)]
    runtime_routing: ScottRuntimeRoutingSettings,
    #[serde(default)]
    bh: ScottBhSettings,
    #[serde(default)]
    ga: ScottGaSettings,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ScottSharedSettings {
    population: Option<usize>,
    seed: Option<u64>,
    temperature: Option<f64>,
    step_size: Option<f64>,
    boundary: Option<f64>,
    collapse: Option<f64>,
    fragment: Option<f64>,
    dspecies: Option<f64>,
    output_level: Option<usize>,
    use_top_analysis: Option<bool>,
    use_dreadnaut_keys: Option<bool>,
    hashkey_radius: Option<String>,
    hashkey_radius_const: Option<f64>,
}

type ScottBackendSettings = application::driver_support::ScottBackendSettings;
type ScottRuntimeRoutingSettings = application::driver_support::ScottRuntimeRoutingSettings;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ScottBhSettings {
    bh_steps: Option<usize>,
    method: Option<String>,
    accept: Option<String>,
    dynamic_threshold: Option<usize>,
    moveclass_threshold: Option<usize>,
    n_temperature: Option<usize>,
    sa_temp_scale: Option<f64>,
    n_high_temperature: Option<usize>,
    n_low_temperature: Option<usize>,
    mc_steps_only: Option<bool>,
    prob_switch_atoms: Option<f64>,
    prob_switch_cations: Option<f64>,
    prob_mutate_cluster: Option<f64>,
    prob_twist_cluster: Option<f64>,
    prob_mutate_cell: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ScottGaSettings {
    ga_generations: Option<usize>,
    n_elites: Option<usize>,
    global_min: Option<f64>,
    tournament_size: Option<usize>,
    reinsert_elites_ratio: Option<f64>,
    pop_replacement_ratio: Option<f64>,
    mutation_ratio: Option<f64>,
    mutate_selfcross_ratio: Option<f64>,
    ga_step_size: Option<f64>,
    cross_check: Option<bool>,
    cross_attempts: Option<usize>,
    cross_1_2d_ratio: Option<f64>,
    gen_stats: Option<bool>,
    save_pop_freq: Option<usize>,
    save_pop_out_format: Option<String>,
    enforce_min_size: Option<f64>,
    ini_pop_attempts: Option<usize>,
    energy_tolerance: Option<f64>,
    cmpr_pmoi: Option<bool>,
    pmoi_tolerance: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
struct ResolvedScottSearchRun {
    job_type: application::legacy_scott::LegacyScottJobType,
    trace_mode: Option<SearchMode>,
    scott_bin: PathBuf,
    data_dir: PathBuf,
    workdir: PathBuf,
    run_dir: PathBuf,
    system: String,
    timeout_secs: Option<u64>,
    shared: ScottSharedSettings,
    backend: ScottBackendSettings,
    runtime_routing: ScottRuntimeRoutingSettings,
    bh: ScottBhSettings,
    ga: ScottGaSettings,
    config_path: Option<PathBuf>,
    cli_overrides: Vec<String>,
}

impl ResolvedScottSearchRun {
    pub(crate) fn trace_mode_label(&self) -> Option<&'static str> {
        self.trace_mode.map(SearchMode::as_str)
    }
}

#[derive(Debug, Parser)]
struct EvaluateScottArgs {
    /// Path to the direct libgulp-linked `klmc_scott` executable.
    #[arg(long)]
    scott_bin: PathBuf,
    /// Path to the Master.gin template containing the KLMC3-RS injection marker.
    #[arg(long)]
    master_gin_template: PathBuf,
    /// Path to the SCOTT run.job template.
    #[arg(long)]
    run_job_template: PathBuf,
    /// Path to the SCOTT atoms.in template.
    #[arg(long)]
    atoms_in_template: PathBuf,
    /// Optional path to the jobs file template.
    #[arg(long)]
    jobs_template: Option<PathBuf>,
    /// Candidate JSON file matching `patina_types::Candidate`.
    #[arg(long)]
    candidate_json: PathBuf,
    /// Sandbox directory used for this evaluation.
    #[arg(long)]
    workdir: PathBuf,
    /// Optional tracked run directory under `runs/active/` or `runs/archive/`.
    #[arg(long)]
    run_dir: Option<PathBuf>,
    /// Search mode label used for manifest and trace export.
    #[arg(long, default_value = "bh")]
    mode: String,
    /// Optional system label written into the run manifest.
    #[arg(long, default_value = "unknown-system")]
    system: String,
    /// Optional timeout in seconds.
    #[arg(long)]
    timeout_secs: Option<u64>,
}

#[derive(Debug, Parser)]
struct EvaluateBackendArgs {
    /// Backend to use for this single-candidate evaluation.
    #[arg(long, value_enum)]
    backend: EvalBackendKind,
    /// Candidate JSON file matching `patina_types::Candidate`.
    #[arg(long)]
    candidate_json: PathBuf,
    /// Sandbox directory used for this evaluation.
    #[arg(long)]
    workdir: PathBuf,
    /// Optional tracked run directory under `runs/active/` or `runs/archive/`.
    #[arg(long)]
    run_dir: Option<PathBuf>,
    /// Search mode label used for manifest and trace export.
    #[arg(long, default_value = "bh")]
    mode: String,
    /// Optional system label written into the run manifest.
    #[arg(long, default_value = "unknown-system")]
    system: String,
    /// Optional timeout in seconds.
    #[arg(long)]
    timeout_secs: Option<u64>,
    /// Path to the GULP or SCOTT executable, depending on backend.
    #[arg(long)]
    executable: Option<PathBuf>,
    /// Path to the Master.gin template containing the KLMC3-RS injection marker.
    #[arg(long)]
    master_gin_template: Option<PathBuf>,
    /// Path to the SCOTT run.job template.
    #[arg(long)]
    run_job_template: Option<PathBuf>,
    /// Path to the SCOTT atoms.in template.
    #[arg(long)]
    atoms_in_template: Option<PathBuf>,
    /// Optional path to the SCOTT jobs file template.
    #[arg(long)]
    jobs_template: Option<PathBuf>,
    /// Python executable used for the Janus adapter.
    #[arg(long)]
    python_bin: Option<PathBuf>,
    /// Python adapter script used for the Janus/MACE backend.
    #[arg(long)]
    janus_adapter_script: Option<PathBuf>,
    /// Janus architecture, for example `mace_mp`.
    #[arg(long, default_value = "mace_mp")]
    janus_arch: String,
    /// Janus model card or local model path.
    #[arg(long, default_value = "small")]
    janus_model: String,
    /// Janus device such as `cpu`, `cuda`, or `mps`.
    #[arg(long, default_value = "cpu")]
    janus_device: String,
    /// Janus calculator dtype.
    #[arg(long, default_value = "float64")]
    janus_dtype: String,
    /// Janus evaluation mode.
    #[arg(long, default_value = "single-point")]
    janus_mode: DriverJanusMode,
    /// Janus local optimization force threshold.
    #[arg(long, default_value_t = 0.1)]
    janus_fmax: f64,
    /// Janus local optimization step limit.
    #[arg(long, default_value_t = 1000)]
    janus_steps: usize,
    /// Janus local optimization algorithm.
    #[arg(long, default_value = "abc-fire")]
    janus_optimizer: DriverJanusOptimizer,
}

#[derive(Debug, Parser)]
struct EvaluateScottStagedArgs {
    /// Candidate JSON file matching `patina_types::Candidate`.
    #[arg(long)]
    candidate_json: PathBuf,
    /// Sandbox directory used for this staged evaluation.
    #[arg(long)]
    workdir: PathBuf,
    /// Directory containing Scott-native staged inputs such as `run.job`, `Master.gin`, and optional `atoms.in`.
    #[arg(long)]
    scott_input_dir: Option<PathBuf>,
    /// Optional tracked run directory under `runs/active/` or `runs/archive/`.
    #[arg(long)]
    run_dir: Option<PathBuf>,
    /// Search mode label used for manifest and trace export.
    #[arg(long, default_value = "bh")]
    mode: String,
    /// Optional system label written into the run manifest.
    #[arg(long, default_value = "unknown-system")]
    system: String,
    /// Optional timeout in seconds.
    #[arg(long)]
    timeout_secs: Option<u64>,
    /// Path to the standalone GULP executable used by the staged runtime when `gulp` is routed.
    #[arg(long)]
    executable: Option<PathBuf>,
    /// Path to the Master.gin template containing the KLMC3-RS injection marker.
    #[arg(long)]
    master_gin_template: Option<PathBuf>,
    /// Path to the SCOTT run.job template used to build the staged evaluator procedure.
    #[arg(long)]
    run_job_template: Option<PathBuf>,
    /// Optional path to the SCOTT atoms.in template used for Scott-shaped staged GULP runs.
    #[arg(long)]
    atoms_in_template: Option<PathBuf>,
    /// Default runtime backend mode for staged evaluation.
    #[arg(long, default_value = "gulp")]
    runtime_default_backend: String,
    /// Optional per-stage backend override in the form `<stage>=<backend>`, for example `1=janus_mace`.
    #[arg(long)]
    runtime_stage_backend: Vec<String>,
    /// Python executable used for the Janus adapter.
    #[arg(long)]
    python_bin: Option<PathBuf>,
    /// Python adapter script used for the Janus/MACE backend.
    #[arg(long)]
    janus_adapter_script: Option<PathBuf>,
    /// Janus architecture, for example `mace_mp`.
    #[arg(long, default_value = "mace_mp")]
    janus_arch: String,
    /// Janus model card or local model path.
    #[arg(long, default_value = "small")]
    janus_model: String,
    /// Janus device such as `cpu`, `cuda`, or `mps`.
    #[arg(long, default_value = "cpu")]
    janus_device: String,
    /// Janus calculator dtype.
    #[arg(long, default_value = "float64")]
    janus_dtype: String,
    /// Janus evaluation mode.
    #[arg(long, default_value = "single-point")]
    janus_mode: DriverJanusMode,
    /// Janus local optimization force threshold.
    #[arg(long, default_value_t = 0.1)]
    janus_fmax: f64,
    /// Janus local optimization step limit.
    #[arg(long, default_value_t = 1000)]
    janus_steps: usize,
    /// Janus local optimization algorithm.
    #[arg(long, default_value = "abc-fire")]
    janus_optimizer: DriverJanusOptimizer,
}

#[derive(Debug, Parser)]
struct RunEnergyLidArgs {
    /// Source GA run directory.
    ///
    /// Supported sources:
    /// - Rust-owned GA runs with `raw/generation_*_state.json`
    /// - exported `legacy-scott` GA runs with `raw/top_structures_*` plus `outputs/structures`
    #[arg(long)]
    source_run_dir: PathBuf,
    /// Tracked run directory for energy-lid outputs.
    #[arg(long)]
    run_dir: PathBuf,
    /// Sandbox directory used for backend evaluations during energy-lid search.
    #[arg(long)]
    workdir: PathBuf,
    /// Optional system label written into the run manifest and summary.
    #[arg(long, default_value = "unknown-system")]
    system: String,
    /// Backend to use for energy-lid evaluations.
    #[arg(long, value_enum, default_value = "gulp")]
    backend: EvalBackendKind,
    /// Number of starting structures selected from the source GA run.
    #[arg(long, default_value_t = 8)]
    top_n: usize,
    /// Number of lid windows to evaluate per starting structure.
    #[arg(long, default_value_t = 8)]
    lid_levels: usize,
    /// Absolute energy increment per lid window, in backend energy units.
    #[arg(long, default_value_t = 1.0)]
    lid_increment: f64,
    /// Monte Carlo steps sampled within each lid window.
    #[arg(long, default_value_t = 20)]
    steps_per_lid: usize,
    /// Monte Carlo displacement amplitude used in lid and quench walks.
    #[arg(long, default_value_t = 0.1)]
    step_size: f64,
    /// Clamp 0D Monte Carlo proposals to a symmetric cluster container like native SCOTT.
    #[arg(long, default_value_t = false)]
    enforce_container: bool,
    /// Optional half-width of the 0D cluster container when container enforcement is enabled.
    #[arg(long)]
    boundary: Option<f64>,
    /// Downhill quench steps sampled in each runner after a lid walk.
    #[arg(long, default_value_t = 10)]
    quench_steps: usize,
    /// Number of runner/quench attempts launched after each lid walk.
    #[arg(long, default_value_t = 3)]
    runners_per_lid: usize,
    /// Random seed for the Rust energy-lid controller.
    #[arg(long, default_value_t = 7)]
    seed: u64,
    /// Optional timeout in seconds.
    #[arg(long)]
    timeout_secs: Option<u64>,
    /// Path to the GULP or SCOTT executable, depending on backend.
    #[arg(long)]
    executable: Option<PathBuf>,
    /// Path to the Master.gin template containing the KLMC3-RS injection marker.
    #[arg(long)]
    master_gin_template: Option<PathBuf>,
    /// Path to the SCOTT run.job template.
    #[arg(long)]
    run_job_template: Option<PathBuf>,
    /// Path to the SCOTT atoms.in template.
    #[arg(long)]
    atoms_in_template: Option<PathBuf>,
    /// Optional path to the SCOTT jobs file template.
    #[arg(long)]
    jobs_template: Option<PathBuf>,
    /// Python executable used for the Janus adapter.
    #[arg(long)]
    python_bin: Option<PathBuf>,
    /// Python adapter script used for the Janus/MACE backend.
    #[arg(long)]
    janus_adapter_script: Option<PathBuf>,
    /// Janus architecture, for example `mace_mp`.
    #[arg(long, default_value = "mace_mp")]
    janus_arch: String,
    /// Janus model card or local model path.
    #[arg(long, default_value = "small")]
    janus_model: String,
    /// Janus device such as `cpu`, `cuda`, or `mps`.
    #[arg(long, default_value = "cpu")]
    janus_device: String,
    /// Janus calculator dtype.
    #[arg(long, default_value = "float64")]
    janus_dtype: String,
    /// Janus evaluation mode.
    #[arg(long, default_value = "single-point")]
    janus_mode: DriverJanusMode,
    /// Janus local optimization force threshold.
    #[arg(long, default_value_t = 0.1)]
    janus_fmax: f64,
    /// Janus local optimization step limit.
    #[arg(long, default_value_t = 1000)]
    janus_steps: usize,
    /// Janus local optimization algorithm.
    #[arg(long, default_value = "abc-fire")]
    janus_optimizer: DriverJanusOptimizer,
}

#[derive(Debug, Parser)]
struct RunBasinHoppingArgs {
    /// Base candidate JSON used as the seed structure for basin hopping.
    #[arg(long)]
    candidate_json: PathBuf,
    /// Tracked run directory for basin-hopping outputs.
    #[arg(long)]
    run_dir: PathBuf,
    /// Sandbox directory used for backend evaluations during basin hopping.
    #[arg(long)]
    workdir: PathBuf,
    /// Optional system label written into the run manifest and summary.
    #[arg(long, default_value = "unknown-system")]
    system: String,
    /// Backend to use for basin-hopping evaluations.
    #[arg(long, value_enum, default_value = "gulp")]
    backend: EvalBackendKind,
    /// Number of basin-hopping steps.
    #[arg(long, default_value_t = 40)]
    bh_steps: usize,
    /// Number of concurrent walkers tracked in the population.
    #[arg(long, default_value_t = 1)]
    walkers: usize,
    /// Basin-hopping temperature.
    #[arg(long, default_value_t = 10.0)]
    temperature: f64,
    /// Basin-hopping displacement amplitude.
    #[arg(long, default_value_t = 0.1)]
    step_size: f64,
    /// Scott BH method semantics for the Rust controller.
    #[arg(long, value_enum, default_value = "relax")]
    bh_method: DriverBhMethodCli,
    /// Scott BH acceptance rule for the Rust controller.
    #[arg(long, value_enum, default_value = "metropolis")]
    bh_accept: DriverBhAcceptCli,
    /// Rejection threshold before the BH controller increases step size.
    #[arg(long)]
    dynamic_threshold: Option<usize>,
    /// Rejection threshold before the BH controller unlocks richer move classes.
    #[arg(long)]
    moveclass_threshold: Option<usize>,
    /// Maximum multiplier allowed for dynamic BH step-size growth.
    #[arg(long)]
    max_dynamic_step_multiplier: Option<f64>,
    /// Oscillatory BH high-temperature window; required when `--bh-method oscillate`.
    #[arg(long)]
    n_high_temperature: Option<usize>,
    /// Oscillatory BH low-temperature window; required when `--bh-method oscillate`.
    #[arg(long)]
    n_low_temperature: Option<usize>,
    /// Probability of atom swapping after richer move classes unlock.
    #[arg(long)]
    prob_switch_atoms: Option<f64>,
    /// Probability of cation swapping after richer move classes unlock.
    #[arg(long)]
    prob_switch_cations: Option<f64>,
    /// Probability of cluster mutation after richer move classes unlock.
    #[arg(long)]
    prob_mutate_cluster: Option<f64>,
    /// Probability of cluster twisting after richer move classes unlock.
    #[arg(long)]
    prob_twist_cluster: Option<f64>,
    /// Probability of cluster translation after richer move classes unlock.
    #[arg(long)]
    prob_translate_cluster: Option<f64>,
    /// Probability of cluster rotation after richer move classes unlock.
    #[arg(long)]
    prob_rotate_cluster: Option<f64>,
    /// Clamp 0D basin-hopping proposals to a symmetric cluster container like native SCOTT.
    #[arg(long, default_value_t = false)]
    enforce_container: bool,
    /// Optional half-width of the 0D cluster container when container enforcement is enabled.
    #[arg(long)]
    boundary: Option<f64>,
    /// Random seed for the Rust basin-hopping controller.
    #[arg(long, default_value_t = 11)]
    seed: u64,
    /// Optional timeout in seconds.
    #[arg(long)]
    timeout_secs: Option<u64>,
    /// Path to the GULP or SCOTT executable, depending on backend.
    #[arg(long)]
    executable: Option<PathBuf>,
    /// Path to the Master.gin template containing the KLMC3-RS injection marker.
    #[arg(long)]
    master_gin_template: Option<PathBuf>,
    /// Path to the SCOTT run.job template.
    #[arg(long)]
    run_job_template: Option<PathBuf>,
    /// Path to the SCOTT atoms.in template.
    #[arg(long)]
    atoms_in_template: Option<PathBuf>,
    /// Optional path to the SCOTT jobs file template.
    #[arg(long)]
    jobs_template: Option<PathBuf>,
    /// Python executable used for the Janus adapter.
    #[arg(long)]
    python_bin: Option<PathBuf>,
    /// Python adapter script used for the Janus/MACE backend.
    #[arg(long)]
    janus_adapter_script: Option<PathBuf>,
    /// Janus architecture, for example `mace_mp`.
    #[arg(long, default_value = "mace_mp")]
    janus_arch: String,
    /// Janus model card or local model path.
    #[arg(long, default_value = "small")]
    janus_model: String,
    /// Janus device such as `cpu`, `cuda`, or `mps`.
    #[arg(long, default_value = "cpu")]
    janus_device: String,
    /// Janus calculator dtype.
    #[arg(long, default_value = "float64")]
    janus_dtype: String,
    /// Janus evaluation mode.
    #[arg(long, default_value = "local-opt")]
    janus_mode: DriverJanusMode,
    /// Janus local optimization force threshold.
    #[arg(long, default_value_t = 0.1)]
    janus_fmax: f64,
    /// Janus local optimization step limit.
    #[arg(long, default_value_t = 1000)]
    janus_steps: usize,
    /// Janus local optimization algorithm.
    #[arg(long, default_value = "abc-fire")]
    janus_optimizer: DriverJanusOptimizer,
}

#[derive(Debug, Parser)]
struct RunSimulatedAnnealingArgs {
    /// Source GA run directory.
    ///
    /// Supported sources:
    /// - Rust-owned GA runs with `raw/generation_*_state.json`
    /// - exported `legacy-scott` GA runs with `raw/top_structures_*` plus `outputs/structures`
    #[arg(long)]
    source_run_dir: PathBuf,
    /// Tracked run directory for simulated annealing outputs.
    #[arg(long)]
    run_dir: PathBuf,
    /// Sandbox directory used for backend evaluations during simulated annealing.
    #[arg(long)]
    workdir: PathBuf,
    /// Optional system label written into the run manifest and summary.
    #[arg(long, default_value = "unknown-system")]
    system: String,
    /// Backend to use for simulated annealing evaluations.
    #[arg(long, value_enum, default_value = "gulp")]
    backend: EvalBackendKind,
    /// Number of starting structures selected from the source GA run.
    #[arg(long, default_value_t = 8)]
    top_n: usize,
    /// Number of annealing steps per starting structure.
    #[arg(long, default_value_t = 40)]
    anneal_steps: usize,
    /// Initial annealing temperature.
    #[arg(long, default_value_t = 25.0)]
    initial_temperature: f64,
    /// Temperature scaling factor applied after each hold window.
    #[arg(long, default_value_t = 0.8)]
    temperature_scale: f64,
    /// Number of accepted/rejected updates to hold each temperature level.
    #[arg(long, default_value_t = 1)]
    hold_steps: usize,
    /// Downhill quench steps sampled after annealing completes.
    #[arg(long, default_value_t = 10)]
    quench_steps: usize,
    /// Monte Carlo displacement amplitude used in annealing and quench walks.
    #[arg(long, default_value_t = 0.1)]
    step_size: f64,
    /// Clamp 0D Monte Carlo proposals to a symmetric cluster container like native SCOTT.
    #[arg(long, default_value_t = false)]
    enforce_container: bool,
    /// Optional half-width of the 0D cluster container when container enforcement is enabled.
    #[arg(long)]
    boundary: Option<f64>,
    /// Random seed for the Rust simulated annealing controller.
    #[arg(long, default_value_t = 17)]
    seed: u64,
    /// Optional timeout in seconds.
    #[arg(long)]
    timeout_secs: Option<u64>,
    /// Path to the GULP or SCOTT executable, depending on backend.
    #[arg(long)]
    executable: Option<PathBuf>,
    /// Path to the Master.gin template containing the KLMC3-RS injection marker.
    #[arg(long)]
    master_gin_template: Option<PathBuf>,
    /// Path to the SCOTT run.job template.
    #[arg(long)]
    run_job_template: Option<PathBuf>,
    /// Path to the SCOTT atoms.in template.
    #[arg(long)]
    atoms_in_template: Option<PathBuf>,
    /// Optional path to the SCOTT jobs file template.
    #[arg(long)]
    jobs_template: Option<PathBuf>,
    /// Python executable used for the Janus adapter.
    #[arg(long)]
    python_bin: Option<PathBuf>,
    /// Python adapter script used for the Janus/MACE backend.
    #[arg(long)]
    janus_adapter_script: Option<PathBuf>,
    /// Janus architecture, for example `mace_mp`.
    #[arg(long, default_value = "mace_mp")]
    janus_arch: String,
    /// Janus model card or local model path.
    #[arg(long, default_value = "small")]
    janus_model: String,
    /// Janus device such as `cpu`, `cuda`, or `mps`.
    #[arg(long, default_value = "cpu")]
    janus_device: String,
    /// Janus calculator dtype.
    #[arg(long, default_value = "float64")]
    janus_dtype: String,
    /// Janus evaluation mode.
    #[arg(long, default_value = "single-point")]
    janus_mode: DriverJanusMode,
    /// Janus local optimization force threshold.
    #[arg(long, default_value_t = 0.1)]
    janus_fmax: f64,
    /// Janus local optimization step limit.
    #[arg(long, default_value_t = 1000)]
    janus_steps: usize,
    /// Janus local optimization algorithm.
    #[arg(long, default_value = "abc-fire")]
    janus_optimizer: DriverJanusOptimizer,
}

#[derive(Debug, Parser)]
struct RunScanSurfaceArgs {
    /// Initial cluster or supported structure candidate JSON.
    #[arg(long)]
    initial_candidate_json: PathBuf,
    /// Optional restart seed candidate JSON. Repeat to scan multiple restart seeds.
    #[arg(long)]
    restart_candidate_json: Vec<PathBuf>,
    /// Tracked run directory for scan-surface outputs.
    #[arg(long)]
    run_dir: PathBuf,
    /// Sandbox directory used for backend evaluations during scan-surface execution.
    #[arg(long)]
    workdir: PathBuf,
    /// Optional system label written into the run manifest and summary.
    #[arg(long, default_value = "unknown-system")]
    system: String,
    /// Backend to use for scan-surface evaluations.
    #[arg(long, value_enum, default_value = "gulp")]
    backend: EvalBackendKind,
    /// Monte Carlo steps per scan seed.
    #[arg(long, default_value_t = 40)]
    steps_per_scan: usize,
    /// Scan-surface acceptance temperature.
    #[arg(long, default_value_t = 300.0)]
    temperature: f64,
    /// Base translation step size.
    #[arg(long, default_value_t = 0.1)]
    base_step_size: f64,
    /// Rejection threshold before increasing dynamic step size.
    #[arg(long, default_value_t = 50)]
    dynamic_threshold: usize,
    /// SCOTT ScanBox move mode.
    #[arg(long, value_enum, default_value = "translate-cluster")]
    move_mode: DriverScanSurfaceMoveCli,
    /// SCOTT ScanBox acceptance mode.
    #[arg(long, value_enum, default_value = "metropolis")]
    acceptance_mode: DriverScanSurfaceAcceptCli,
    /// Evaluate and seed the initial state before sampling proposals.
    #[arg(long, default_value_t = false)]
    evaluate_initial_state: bool,
    /// Scan-box center x coordinate.
    #[arg(long, default_value_t = 0.0)]
    center_x: f64,
    /// Scan-box center y coordinate.
    #[arg(long, default_value_t = 0.0)]
    center_y: f64,
    /// Scan-box center z coordinate.
    #[arg(long, default_value_t = 0.0)]
    center_z: f64,
    /// Scan-box half-width on x.
    #[arg(long, default_value_t = 10.0)]
    boundary_x: f64,
    /// Scan-box half-width on y.
    #[arg(long, default_value_t = 10.0)]
    boundary_y: f64,
    /// Scan-box half-width on z.
    #[arg(long, default_value_t = 10.0)]
    boundary_z: f64,
    /// Keep randomized/translated clusters above the configured surface plane.
    #[arg(long, default_value_t = false)]
    above_surface: bool,
    /// Random seed for the Rust scan-surface controller.
    #[arg(long, default_value_t = 23)]
    seed: u64,
    /// Optional timeout in seconds.
    #[arg(long)]
    timeout_secs: Option<u64>,
    /// Path to the GULP or SCOTT executable, depending on backend.
    #[arg(long)]
    executable: Option<PathBuf>,
    /// Path to the Master.gin template containing the KLMC3-RS injection marker.
    #[arg(long)]
    master_gin_template: Option<PathBuf>,
    /// Path to the SCOTT run.job template.
    #[arg(long)]
    run_job_template: Option<PathBuf>,
    /// Path to the SCOTT atoms.in template.
    #[arg(long)]
    atoms_in_template: Option<PathBuf>,
    /// Optional path to the SCOTT jobs file template.
    #[arg(long)]
    jobs_template: Option<PathBuf>,
    /// Python executable used for the Janus adapter.
    #[arg(long)]
    python_bin: Option<PathBuf>,
    /// Python adapter script used for the Janus/MACE backend.
    #[arg(long)]
    janus_adapter_script: Option<PathBuf>,
    /// Janus architecture, for example `mace_mp`.
    #[arg(long, default_value = "mace_mp")]
    janus_arch: String,
    /// Janus model card or local model path.
    #[arg(long, default_value = "small")]
    janus_model: String,
    /// Janus device such as `cpu`, `cuda`, or `mps`.
    #[arg(long, default_value = "cpu")]
    janus_device: String,
    /// Janus calculator dtype.
    #[arg(long, default_value = "float64")]
    janus_dtype: String,
    /// Janus evaluation mode.
    #[arg(long, default_value = "single-point")]
    janus_mode: DriverJanusMode,
    /// Janus local optimization force threshold.
    #[arg(long, default_value_t = 0.1)]
    janus_fmax: f64,
    /// Janus local optimization step limit.
    #[arg(long, default_value_t = 1000)]
    janus_steps: usize,
    /// Janus local optimization algorithm.
    #[arg(long, default_value = "abc-fire")]
    janus_optimizer: DriverJanusOptimizer,
}

#[derive(Debug, Parser)]
struct RunSolidSolutionsArgs {
    /// Initial solid-solution candidate JSON.
    #[arg(long)]
    initial_candidate_json: PathBuf,
    /// Optional explicit proposal candidate JSON. Repeat to screen a fixed proposal sequence.
    #[arg(long)]
    proposal_candidate_json: Vec<PathBuf>,
    /// Tracked run directory for solid-solution outputs.
    #[arg(long)]
    run_dir: PathBuf,
    /// Sandbox directory used for backend evaluations and hashkey scratch files.
    #[arg(long)]
    workdir: PathBuf,
    /// Optional system label written into the run manifest and summary.
    #[arg(long, default_value = "unknown-system")]
    system: String,
    /// Backend to use for solid-solution evaluations.
    #[arg(long, value_enum, default_value = "gulp")]
    backend: EvalBackendKind,
    /// Generated solid-solution steps when no explicit proposals are supplied.
    #[arg(long, default_value_t = 40)]
    steps: usize,
    /// Solid-solution acceptance temperature.
    #[arg(long, default_value_t = 300.0)]
    temperature: f64,
    /// SCOTT solid-solution move mode.
    #[arg(long, value_enum, default_value = "mix-solution")]
    move_mode: DriverSolidSolutionsMoveCli,
    /// SCOTT solid-solution acceptance mode.
    #[arg(long, value_enum, default_value = "metropolis")]
    acceptance_mode: DriverSolidSolutionsAcceptCli,
    /// Maximum exchanges per generated mix-solution step.
    #[arg(long, default_value_t = 1)]
    max_exchanges: usize,
    /// Skip backend energy evaluation while still running geometry/hashkey screening.
    #[arg(long, default_value_t = false)]
    skip_evaluation: bool,
    /// Imported hashkey from a previous solid-solution library. Repeat as needed.
    #[arg(long)]
    imported_hashkey: Vec<String>,
    /// File containing imported hashkeys, one per line or legacy `TYPE,Hashkey,Occurance` CSV.
    #[arg(long)]
    imported_hashkeys_file: Vec<PathBuf>,
    /// Enable Dreadnaut hashkeys for duplicate screening and imported-library comparisons.
    #[arg(long)]
    use_dreadnaut_keys: bool,
    /// Path to the SCOTT atoms.in template used for backend sidecars and native hashkey radii.
    #[arg(long)]
    atoms_in_template: Option<PathBuf>,
    /// Native-compatible hashkey radius mode.
    #[arg(long, default_value = "IR")]
    hashkey_radius: String,
    /// Extra hashkey radius constant, matching native SCOTT conventions.
    #[arg(long, default_value_t = 0.0)]
    hashkey_radius_const: f64,
    /// Optional minimum pair distance used as a conservative geometry gate.
    #[arg(long)]
    geometry_min_distance: Option<f64>,
    /// Write legacy solid-solution statistics artifacts (`ssStatistics.csv` and hashkey stats).
    #[arg(long, default_value_t = false)]
    ss_stats: bool,
    /// Write legacy-style `ssStatistics<step>.csv` backup snapshots at this interval.
    #[arg(long, default_value_t = 0)]
    ss_stats_backup: usize,
    /// Compute solid-solution D-RDF/T-RDF artifacts for evaluated structures.
    #[arg(long, default_value_t = false)]
    compute_rdf: bool,
    /// RDF cutoff, matching native `R_RDF_CUTOFF`.
    #[arg(long, default_value_t = 15.0)]
    rdf_cutoff: f64,
    /// RDF output step. Defaults to `(cutoff - zero) / (steps - 1)`.
    #[arg(long)]
    rdf_step: Option<f64>,
    /// RDF output point count, matching native `N_RDF_STEPS`.
    #[arg(long, default_value_t = 5000)]
    rdf_steps: usize,
    /// RDF Gaussian smearing sigma for T-RDF, matching native `R_RDF_SIGMA`.
    #[arg(long, default_value_t = 0.1)]
    rdf_sigma: f64,
    /// RDF minimum radius, matching native `R_RDF_ZERO`.
    #[arg(long, default_value_t = 0.0)]
    rdf_zero: f64,
    /// RDF unique-distance grouping tolerance, matching native `R_RDF_ACC`.
    #[arg(long, default_value_t = 0.00001)]
    rdf_accuracy: f64,
    /// Random seed for the Rust solid-solution controller.
    #[arg(long, default_value_t = 29)]
    seed: u64,
    /// Optional timeout in seconds.
    #[arg(long)]
    timeout_secs: Option<u64>,
    /// Path to the GULP or SCOTT executable, depending on backend.
    #[arg(long)]
    executable: Option<PathBuf>,
    /// Path to the Master.gin template containing the KLMC3-RS injection marker.
    #[arg(long)]
    master_gin_template: Option<PathBuf>,
    /// Path to the SCOTT run.job template.
    #[arg(long)]
    run_job_template: Option<PathBuf>,
    /// Optional path to the SCOTT jobs file template.
    #[arg(long)]
    jobs_template: Option<PathBuf>,
    /// Python executable used for the Janus adapter.
    #[arg(long)]
    python_bin: Option<PathBuf>,
    /// Python adapter script used for the Janus/MACE backend.
    #[arg(long)]
    janus_adapter_script: Option<PathBuf>,
    /// Janus architecture, for example `mace_mp`.
    #[arg(long, default_value = "mace_mp")]
    janus_arch: String,
    /// Janus model card or local model path.
    #[arg(long, default_value = "small")]
    janus_model: String,
    /// Janus device such as `cpu`, `cuda`, or `mps`.
    #[arg(long, default_value = "cpu")]
    janus_device: String,
    /// Janus calculator dtype.
    #[arg(long, default_value = "float64")]
    janus_dtype: String,
    /// Janus evaluation mode.
    #[arg(long, default_value = "single-point")]
    janus_mode: DriverJanusMode,
    /// Janus local optimization force threshold.
    #[arg(long, default_value_t = 0.1)]
    janus_fmax: f64,
    /// Janus local optimization step limit.
    #[arg(long, default_value_t = 1000)]
    janus_steps: usize,
    /// Janus local optimization algorithm.
    #[arg(long, default_value = "abc-fire")]
    janus_optimizer: DriverJanusOptimizer,
}

#[derive(Debug, Parser)]
struct AnalyzeFrameworkSymmetryArgs {
    /// Typed candidate JSON input. Provide either this or `--structure-path`.
    #[arg(long)]
    candidate_json: Option<PathBuf>,
    /// Periodic structure snapshot such as `.cif`, `.xyz`, `.extxyz`, `.car`, `.arc`, or `.can`.
    #[arg(long)]
    structure_path: Option<PathBuf>,
    /// Tracked run directory for emitted symmetry artifacts.
    #[arg(long)]
    run_dir: PathBuf,
    /// Human-readable system label stored in the run manifest.
    #[arg(long)]
    system: Option<String>,
    /// Symmetry search position tolerance in lattice units.
    #[arg(long, default_value_t = 1.0e-5)]
    position_tolerance: f64,
    /// Reserved cell tolerance kept in the Rust-owned contract for future framework normalization steps.
    #[arg(long, default_value_t = 1.0e-5)]
    cell_tolerance: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum DriverFrameworkNormalizationTarget {
    Standardized,
    PrimitiveStandardized,
}

#[derive(Debug, Parser)]
struct NormalizeFrameworkArgs {
    /// Typed candidate JSON input. Provide either this or `--structure-path`.
    #[arg(long)]
    candidate_json: Option<PathBuf>,
    /// Periodic structure snapshot such as `.cif`, `.xyz`, `.extxyz`, `.car`, `.arc`, or `.can`.
    #[arg(long)]
    structure_path: Option<PathBuf>,
    /// Tracked run directory for emitted normalization artifacts.
    #[arg(long)]
    run_dir: PathBuf,
    /// Human-readable system label stored in the run manifest.
    #[arg(long)]
    system: Option<String>,
    /// Normalization target to emit.
    #[arg(long, value_enum, default_value = "standardized")]
    target: DriverFrameworkNormalizationTarget,
    /// Symmetry search position tolerance in lattice units.
    #[arg(long, default_value_t = 1.0e-5)]
    position_tolerance: f64,
    /// Reserved cell tolerance kept in the Rust-owned contract for future framework normalization steps.
    #[arg(long, default_value_t = 1.0e-5)]
    cell_tolerance: f64,
}

#[derive(Debug, Parser)]
struct RunFrameworkGcmcArgs {
    /// Typed candidate JSON input. Provide either this or `--structure-path`.
    #[arg(long)]
    candidate_json: Option<PathBuf>,
    /// Periodic structure snapshot such as `.cif`, `.xyz`, `.extxyz`, `.car`, `.arc`, or `.can`.
    #[arg(long)]
    structure_path: Option<PathBuf>,
    /// Tracked run directory for emitted GCMC artifacts.
    #[arg(long)]
    run_dir: PathBuf,
    /// Working directory for Janus/MACE evaluation sandboxes.
    #[arg(long)]
    workdir: PathBuf,
    /// Human-readable system label stored in the run manifest.
    #[arg(long)]
    system: Option<String>,
    /// Guest species model. The first milestone currently supports `h2`.
    #[arg(long, default_value = "h2")]
    guest: String,
    /// Temperature in kelvin.
    #[arg(long, default_value_t = 298.0)]
    temperature_kelvin: f64,
    /// Pressure in bar.
    #[arg(long, default_value_t = 1.0)]
    pressure_bar: f64,
    /// Initialization cycles discarded before production statistics are accumulated.
    #[arg(long, default_value_t = 200)]
    initialization_cycles: usize,
    /// Production cycles accumulated into loading, energy, and density statistics.
    #[arg(long, default_value_t = 1000)]
    production_cycles: usize,
    /// Maximum number of guest molecules allowed in the box.
    #[arg(long, default_value_t = 32)]
    max_guest_count: usize,
    /// Minimum guest-host distance in angstrom for pre-screening proposed insertions.
    #[arg(long, default_value_t = 1.2)]
    minimum_guest_host_distance: f64,
    /// Minimum guest-guest intermolecular distance in angstrom for pre-screening.
    #[arg(long, default_value_t = 1.0)]
    minimum_guest_guest_distance: f64,
    /// Density-grid x dimension.
    #[arg(long, default_value_t = 48)]
    density_nx: usize,
    /// Density-grid y dimension.
    #[arg(long, default_value_t = 48)]
    density_ny: usize,
    /// Density-grid z dimension.
    #[arg(long, default_value_t = 48)]
    density_nz: usize,
    /// Density-grid binning mode.
    #[arg(long, value_enum, default_value = "equitable")]
    density_binning: DriverDensityGridBinning,
    /// Density-grid normalization mode.
    #[arg(long, value_enum, default_value = "number-density")]
    density_normalization: DriverDensityGridNormalization,
    /// Energy histogram bin count.
    #[arg(long, default_value_t = 128)]
    energy_histogram_bins: usize,
    /// Lower energy histogram bound in eV relative to the bare framework energy.
    #[arg(long, default_value_t = -5.0)]
    energy_histogram_min_ev: f64,
    /// Upper energy histogram bound in eV relative to the bare framework energy.
    #[arg(long, default_value_t = 1.0)]
    energy_histogram_max_ev: f64,
    /// Number-histogram lower occupancy limit.
    #[arg(long, default_value_t = 0)]
    number_histogram_lower: usize,
    /// Number-histogram upper occupancy limit.
    #[arg(long, default_value_t = 32)]
    number_histogram_upper: usize,
    /// Optional deterministic random seed.
    #[arg(long, default_value_t = 11)]
    seed: u64,
    /// Python executable used for the Janus adapter.
    #[arg(long)]
    python_bin: Option<PathBuf>,
    /// Python adapter script used for the Janus/MACE backend.
    #[arg(long)]
    janus_adapter_script: Option<PathBuf>,
    /// Janus architecture, for example `mace_mp`.
    #[arg(long, default_value = "mace_mp")]
    janus_arch: String,
    /// Janus model card or local model path.
    #[arg(long, default_value = "small")]
    janus_model: String,
    /// Janus device such as `cpu`, `cuda`, or `mps`.
    #[arg(long, default_value = "cpu")]
    janus_device: String,
    /// Janus calculator dtype.
    #[arg(long, default_value = "float64")]
    janus_dtype: String,
    /// Janus evaluation mode. For framework GCMC this should stay `single-point`.
    #[arg(long, value_enum, default_value = "single-point")]
    janus_mode: DriverJanusMode,
    /// Janus local optimization force threshold.
    #[arg(long, default_value_t = 0.1)]
    janus_fmax: f64,
    /// Janus local optimization step limit.
    #[arg(long, default_value_t = 1000)]
    janus_steps: usize,
    /// Janus local optimization algorithm.
    #[arg(long, default_value = "abc-fire")]
    janus_optimizer: DriverJanusOptimizer,
}

#[derive(Debug, Parser)]
struct StructureArgs {
    #[command(subcommand)]
    command: StructureCommand,
}

#[derive(Debug, Subcommand)]
enum StructureCommand {
    /// Print the space group for a periodic structure or stage directory.
    #[command(name = "space-group", alias = "sg")]
    SpaceGroup(StructureSpaceGroupArgs),
}

#[derive(Debug, Parser)]
struct StructureSpaceGroupArgs {
    /// Optional structure or typed candidate path. If omitted, the current directory is the stage.
    #[arg(value_name = "PATH")]
    path: Option<PathBuf>,
    /// Figment configuration file (`.toml` or `.json`) using stage/input/structure sections.
    #[arg(long)]
    config: Option<PathBuf>,
    /// Stage directory used when no explicit input path is provided. Defaults to the current directory.
    #[arg(long)]
    stage_dir: Option<PathBuf>,
    /// Typed candidate JSON input. Overrides configured structure input.
    #[arg(long)]
    candidate_json: Option<PathBuf>,
    /// Structure snapshot such as `.cif`, `.xyz`, `.extxyz`, `.car`, `.arc`, or `.can`.
    #[arg(long)]
    structure_path: Option<PathBuf>,
    /// Symmetry search position tolerance in lattice units.
    #[arg(long)]
    position_tolerance: Option<f64>,
    /// Reserved cell tolerance kept in the Rust-owned contract for future framework normalization steps.
    #[arg(long)]
    cell_tolerance: Option<f64>,
    /// Print a machine-readable JSON report.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Parser)]
struct ConstructStkZeroDArgs {
    /// Built-in 0D topology preset to construct.
    #[arg(long, value_enum, default_value = "square-planar")]
    topology: DriverStkZeroDTopology,
    /// Output directory for XYZ, summary JSON, and full construction-result JSON artifacts.
    #[arg(long, default_value = "runs/stk-zero-d")]
    output_dir: PathBuf,
    /// Print the construction summary as JSON.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum DriverDensityGridBinning {
    Standard,
    Equitable,
}

impl From<DriverDensityGridBinning> for DensityGridBinning {
    fn from(value: DriverDensityGridBinning) -> Self {
        match value {
            DriverDensityGridBinning::Standard => Self::Standard,
            DriverDensityGridBinning::Equitable => Self::Equitable,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum DriverDensityGridNormalization {
    Max,
    NumberDensity,
}

impl From<DriverDensityGridNormalization> for DensityGridNormalization {
    fn from(value: DriverDensityGridNormalization) -> Self {
        match value {
            DriverDensityGridNormalization::Max => Self::Max,
            DriverDensityGridNormalization::NumberDensity => Self::NumberDensity,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum DriverSurfaceCutStrategy {
    FixedOffset,
    TopologyAware,
}

impl From<DriverSurfaceCutStrategy> for SurfaceCutStrategy {
    fn from(value: DriverSurfaceCutStrategy) -> Self {
        match value {
            DriverSurfaceCutStrategy::FixedOffset => Self::FixedOffset,
            DriverSurfaceCutStrategy::TopologyAware => Self::TopologyAware,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum DriverSurfaceReconstructionMode {
    None,
    IonicBalance,
}

impl From<DriverSurfaceReconstructionMode> for SurfaceReconstructionMode {
    fn from(value: DriverSurfaceReconstructionMode) -> Self {
        match value {
            DriverSurfaceReconstructionMode::None => Self::None,
            DriverSurfaceReconstructionMode::IonicBalance => Self::IonicBalance,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum DriverSurfaceFace {
    Top,
    Bottom,
}

impl From<DriverSurfaceFace> for SurfaceFace {
    fn from(value: DriverSurfaceFace) -> Self {
        match value {
            DriverSurfaceFace::Top => Self::Top,
            DriverSurfaceFace::Bottom => Self::Bottom,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum DriverSurfaceReconstructionSiteFilter {
    DanglingOnly,
    DanglingPreferred,
    SurfaceFaceOnly,
}

impl From<DriverSurfaceReconstructionSiteFilter>
    for application::surface_reconstruction::SurfaceReconstructionSiteFilter
{
    fn from(value: DriverSurfaceReconstructionSiteFilter) -> Self {
        match value {
            DriverSurfaceReconstructionSiteFilter::DanglingOnly => Self::DanglingOnly,
            DriverSurfaceReconstructionSiteFilter::DanglingPreferred => Self::DanglingPreferred,
            DriverSurfaceReconstructionSiteFilter::SurfaceFaceOnly => Self::SurfaceFaceOnly,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum DriverSurfaceReconstructionRegionPolicy {
    FullFace,
    RelaxedRegion,
    TopLayerOnly,
}

impl From<DriverSurfaceReconstructionRegionPolicy>
    for application::surface_reconstruction::SurfaceReconstructionRegionPolicy
{
    fn from(value: DriverSurfaceReconstructionRegionPolicy) -> Self {
        match value {
            DriverSurfaceReconstructionRegionPolicy::FullFace => Self::FullFace,
            DriverSurfaceReconstructionRegionPolicy::RelaxedRegion => Self::RelaxedRegion,
            DriverSurfaceReconstructionRegionPolicy::TopLayerOnly => Self::TopLayerOnly,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum DriverSurfaceReconstructionMoveFamily {
    LateralOnly,
    OutwardNormalOnly,
    LateralAndOutwardNormal,
    LateralAndBidirectionalNormal,
}

impl From<DriverSurfaceReconstructionMoveFamily>
    for application::surface_reconstruction::SurfaceReconstructionMoveFamily
{
    fn from(value: DriverSurfaceReconstructionMoveFamily) -> Self {
        match value {
            DriverSurfaceReconstructionMoveFamily::LateralOnly => Self::LateralOnly,
            DriverSurfaceReconstructionMoveFamily::OutwardNormalOnly => Self::OutwardNormalOnly,
            DriverSurfaceReconstructionMoveFamily::LateralAndOutwardNormal => {
                Self::LateralAndOutwardNormal
            }
            DriverSurfaceReconstructionMoveFamily::LateralAndBidirectionalNormal => {
                Self::LateralAndBidirectionalNormal
            }
        }
    }
}

#[derive(Debug, Parser)]
struct GenerateSurfaceArgs {
    /// Typed candidate JSON input. Provide either this or `--structure-path`.
    #[arg(long)]
    candidate_json: Option<PathBuf>,
    /// Periodic structure snapshot such as `.cif`, `.xyz`, `.extxyz`, `.car`, `.arc`, or `.can`.
    #[arg(long)]
    structure_path: Option<PathBuf>,
    /// Tracked run directory for emitted surface artifacts.
    #[arg(long)]
    run_dir: PathBuf,
    /// Human-readable system label stored in the run manifest.
    #[arg(long)]
    system: Option<String>,
    /// Miller h index.
    #[arg(long)]
    h: i32,
    /// Miller k index.
    #[arg(long)]
    k: i32,
    /// Miller l index.
    #[arg(long)]
    l: i32,
    /// Target slab thickness in angstrom.
    #[arg(long, default_value_t = 10.0)]
    thickness: f64,
    /// Vacuum height in angstrom.
    #[arg(long, default_value_t = 15.0)]
    vacuum: f64,
    /// In-plane repeat count along the first slab vector.
    #[arg(long, default_value_t = 1)]
    supercell_a: usize,
    /// In-plane repeat count along the second slab vector.
    #[arg(long, default_value_t = 1)]
    supercell_b: usize,
    /// Cut strategy for surface termination selection.
    #[arg(long, value_enum, default_value = "topology-aware")]
    cut_strategy: DriverSurfaceCutStrategy,
    /// Optional cut offset expressed as a fraction of d_hkl.
    #[arg(long)]
    cut_offset_fraction: Option<f64>,
    /// Remove duplicate slab atoms after population.
    #[arg(long, default_value_t = false)]
    dedup_slab: bool,
    /// Apply the source project's current in-plane reduction scaffold after population.
    #[arg(long, default_value_t = false)]
    reduce_slab_inplane: bool,
    /// Fractional tolerance for slab deduplication.
    #[arg(long, default_value_t = 2.0e-4)]
    dedup_frac_tol: f64,
    /// If true, wrap z during dedup as well as x/y.
    #[arg(long, default_value_t = false)]
    dedup_wrap_z: bool,
    /// If true, allow different elements to merge during dedup.
    #[arg(long, default_value_t = false)]
    dedup_ignore_element: bool,
}

#[derive(Debug, Parser)]
struct RunSurfaceReconstructionArgs {
    /// Typed candidate JSON input. Provide either this or `--structure-path`.
    #[arg(long)]
    candidate_json: Option<PathBuf>,
    /// Periodic structure snapshot such as `.cif`, `.xyz`, `.extxyz`, `.car`, `.arc`, or `.can`.
    #[arg(long)]
    structure_path: Option<PathBuf>,
    /// Tracked run directory for emitted reconstruction artifacts.
    #[arg(long)]
    run_dir: PathBuf,
    /// Working directory for backend evaluation sandboxes.
    #[arg(long)]
    workdir: PathBuf,
    /// Human-readable system label stored in the run manifest.
    #[arg(long)]
    system: Option<String>,
    /// Backend to use for reconstruction evaluations.
    #[arg(long, value_enum, default_value = "gulp")]
    backend: EvalBackendKind,
    /// Miller h index.
    #[arg(long)]
    h: i32,
    /// Miller k index.
    #[arg(long)]
    k: i32,
    /// Miller l index.
    #[arg(long)]
    l: i32,
    /// Target slab thickness in angstrom.
    #[arg(long, default_value_t = 10.0)]
    thickness: f64,
    /// Vacuum height in angstrom.
    #[arg(long, default_value_t = 15.0)]
    vacuum: f64,
    /// In-plane repeat count along the first slab vector.
    #[arg(long, default_value_t = 1)]
    supercell_a: usize,
    /// In-plane repeat count along the second slab vector.
    #[arg(long, default_value_t = 1)]
    supercell_b: usize,
    /// Cut strategy for surface termination selection.
    #[arg(long, value_enum, default_value = "topology-aware")]
    cut_strategy: DriverSurfaceCutStrategy,
    /// Optional cut offset expressed as a fraction of d_hkl.
    #[arg(long)]
    cut_offset_fraction: Option<f64>,
    /// Remove duplicate slab atoms after population.
    #[arg(long, default_value_t = false)]
    dedup_slab: bool,
    /// Apply the source project's current in-plane reduction scaffold after population.
    #[arg(long, default_value_t = false)]
    reduce_slab_inplane: bool,
    /// Fractional tolerance for slab deduplication.
    #[arg(long, default_value_t = 2.0e-4)]
    dedup_frac_tol: f64,
    /// If true, wrap z during dedup as well as x/y.
    #[arg(long, default_value_t = false)]
    dedup_wrap_z: bool,
    /// If true, allow different elements to merge during dedup.
    #[arg(long, default_value_t = false)]
    dedup_ignore_element: bool,
    /// Surface-generation seed reconstruction mode before Monte Carlo ion moves.
    #[arg(long, value_enum, default_value = "none")]
    reconstruction: DriverSurfaceReconstructionMode,
    /// Surface face to reconstruct.
    #[arg(long, value_enum, default_value = "top")]
    target_face: DriverSurfaceFace,
    /// Restrict movable ions to these species. Repeat as needed.
    #[arg(long)]
    movable_species: Vec<String>,
    /// Site-selection policy for candidate mobile ions on the target face.
    #[arg(long, value_enum, default_value = "dangling-preferred")]
    site_filter: DriverSurfaceReconstructionSiteFilter,
    /// Region-selection policy applied after site classification.
    #[arg(long, value_enum, default_value = "full-face")]
    region_policy: DriverSurfaceReconstructionRegionPolicy,
    /// Number of near-surface layers allowed to move when using `relaxed-region`.
    #[arg(long)]
    movable_layer_count: Option<usize>,
    /// z-grouping tolerance in angstrom for assigning atoms to surface layers.
    #[arg(long, default_value_t = 0.5)]
    layer_z_tolerance_angstrom: f64,
    /// Proposal move family applied to selected ions.
    #[arg(long, value_enum, default_value = "lateral-and-outward-normal")]
    move_family: DriverSurfaceReconstructionMoveFamily,
    /// Monte Carlo reconstruction steps.
    #[arg(long, default_value_t = 40)]
    steps: usize,
    /// Monte Carlo acceptance temperature.
    #[arg(long, default_value_t = 300.0)]
    temperature: f64,
    /// Maximum lateral fractional displacement magnitude per proposal.
    #[arg(long, default_value_t = 0.05)]
    lateral_fractional_step: f64,
    /// Maximum outward normal displacement scale in angstrom.
    #[arg(long, default_value_t = 1.0)]
    outward_normal_step_angstrom: f64,
    /// Maximum inward normal displacement scale in angstrom.
    #[arg(long, default_value_t = 0.25)]
    inward_normal_step_angstrom: f64,
    /// Evaluate the generated slab before the first move.
    #[arg(long, default_value_t = true)]
    evaluate_initial_state: bool,
    /// Number of best-ranked structures to export as JSON and CIF.
    #[arg(long, default_value_t = 10)]
    top_n_exports: usize,
    /// Deterministic seed for the reconstruction controller.
    #[arg(long, default_value_t = 17)]
    seed: u64,
    /// Optional timeout in seconds.
    #[arg(long)]
    timeout_secs: Option<u64>,
    /// Path to the GULP or SCOTT executable, depending on backend.
    #[arg(long)]
    executable: Option<PathBuf>,
    /// Path to the Master.gin template containing the KLMC3-RS injection marker.
    #[arg(long)]
    master_gin_template: Option<PathBuf>,
    /// Path to the SCOTT run.job template.
    #[arg(long)]
    run_job_template: Option<PathBuf>,
    /// Optional path to the SCOTT atoms.in template used for backend sidecars.
    #[arg(long)]
    atoms_in_template: Option<PathBuf>,
    /// Optional path to the SCOTT jobs file template.
    #[arg(long)]
    jobs_template: Option<PathBuf>,
    /// Python executable used for the Janus adapter.
    #[arg(long)]
    python_bin: Option<PathBuf>,
    /// Python adapter script used for the Janus/MACE backend.
    #[arg(long)]
    janus_adapter_script: Option<PathBuf>,
    /// Janus architecture, for example `mace_mp`.
    #[arg(long, default_value = "mace_mp")]
    janus_arch: String,
    /// Janus model card or local model path.
    #[arg(long, default_value = "small")]
    janus_model: String,
    /// Janus device such as `cpu`, `cuda`, or `mps`.
    #[arg(long, default_value = "cpu")]
    janus_device: String,
    /// Janus calculator dtype.
    #[arg(long, default_value = "float64")]
    janus_dtype: String,
    /// Janus evaluation mode.
    #[arg(long, default_value = "single-point")]
    janus_mode: DriverJanusMode,
    /// Janus local optimization force threshold.
    #[arg(long, default_value_t = 0.1)]
    janus_fmax: f64,
    /// Janus local optimization step limit.
    #[arg(long, default_value_t = 1000)]
    janus_steps: usize,
    /// Janus local optimization algorithm.
    #[arg(long, default_value = "abc-fire")]
    janus_optimizer: DriverJanusOptimizer,
}

#[derive(Debug, Parser)]
struct AnalyzeSurfacePolarityArgs {
    /// Typed candidate JSON input. Provide either this or `--structure-path`.
    #[arg(long)]
    candidate_json: Option<PathBuf>,
    /// Structure snapshot such as `.cif`, `.xyz`, `.extxyz`, `.car`, `.arc`, or `.can`.
    #[arg(long)]
    structure_path: Option<PathBuf>,
    /// Tracked run directory for emitted polarity artifacts.
    #[arg(long)]
    run_dir: PathBuf,
    /// Human-readable system label stored in the run manifest.
    #[arg(long)]
    system: Option<String>,
}

#[derive(Debug, Parser)]
struct PerturbClusterArgs {
    /// Typed candidate JSON input for a zero-dimensional cluster.
    #[arg(long)]
    candidate_json: PathBuf,
    /// Tracked run directory for emitted perturbation artifacts.
    #[arg(long)]
    run_dir: PathBuf,
    /// Human-readable system label stored in the run manifest.
    #[arg(long)]
    system: Option<String>,
    /// Number of perturbed variants to generate.
    #[arg(long)]
    count: usize,
    /// Isotropic Gaussian displacement sigma in angstrom.
    #[arg(long)]
    sigma: f64,
    /// Optional hard cap on displacement norm per atom.
    #[arg(long)]
    max_displacement: Option<f64>,
    /// Optional minimum interatomic distance required after perturbation.
    #[arg(long)]
    validate_min_distance: Option<f64>,
    /// Duplicate-screening threshold applied between the source and each generated variant.
    #[arg(long, default_value_t = 1.0e-6)]
    duplicate_threshold: f64,
    /// Duplicate-screening engine used for source-vs-variant comparison.
    #[arg(long, value_enum, default_value_t = PerturbClusterDuplicateScreeningModeCli::GlobalOverlap)]
    duplicate_screening_mode: PerturbClusterDuplicateScreeningModeCli,
    /// Include p-orbital channels in the overlap-matrix fingerprint.
    #[arg(long, default_value_t = false)]
    include_p_orbitals: bool,
    /// Environment-fingerprint cutoff radius in angstrom when environment-assignment screening is selected.
    #[arg(long, default_value_t = 5.0)]
    environment_width_cutoff: f64,
    /// Maximum number of atoms retained in each local environment sphere when environment-assignment screening is selected.
    #[arg(long, default_value_t = 100)]
    environment_max_atoms_in_sphere: usize,
    /// Number of s orbitals per atom in the environment-assignment screening basis.
    #[arg(long, default_value_t = 1)]
    environment_s_orbital_count: usize,
    /// Number of p-orbital triplets per atom in the environment-assignment screening basis.
    #[arg(long, default_value_t = 1)]
    environment_p_orbital_count: usize,
    /// Optional deterministic random seed for perturbation generation.
    #[arg(long)]
    seed: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum PerturbClusterDuplicateScreeningModeCli {
    GlobalOverlap,
    EnvironmentAssignment,
}

#[derive(Debug, Parser)]
struct RunScottProductionArgs {
    /// Candidate JSON files matching `patina_types::Candidate`.
    #[arg(long)]
    candidate_json: Vec<PathBuf>,
    /// Native-style restart directory containing seed structures such as `.xyz`, `.extxyz`, `.cif`, `.car`, `.arc`, or `.can`.
    #[arg(long)]
    restart_dir: Option<PathBuf>,
    /// Base workdir used for this staged production-style run.
    #[arg(long)]
    workdir: PathBuf,
    /// Directory containing Scott-native staged inputs such as `run.job`, `Master.gin`, and optional `atoms.in`.
    #[arg(long)]
    scott_input_dir: Option<PathBuf>,
    /// Optional tracked run directory under `runs/active/` or `runs/archive/`.
    #[arg(long)]
    run_dir: Option<PathBuf>,
    /// Optional system label written into the run manifest.
    #[arg(long, default_value = "unknown-system")]
    system: String,
    /// Optional timeout in seconds.
    #[arg(long)]
    timeout_secs: Option<u64>,
    /// Path to the standalone GULP executable used by the staged runtime when `gulp` is routed.
    #[arg(long)]
    executable: Option<PathBuf>,
    /// Path to the Master.gin template containing the KLMC3-RS injection marker.
    #[arg(long)]
    master_gin_template: Option<PathBuf>,
    /// Path to the SCOTT run.job template used to build the staged evaluator procedure.
    #[arg(long)]
    run_job_template: Option<PathBuf>,
    /// Optional path to the SCOTT atoms.in template used for Scott-shaped staged GULP runs.
    #[arg(long)]
    atoms_in_template: Option<PathBuf>,
    /// Default runtime backend mode for staged evaluation.
    #[arg(long, default_value = "gulp")]
    runtime_default_backend: String,
    /// Optional per-stage backend override in the form `<stage>=<backend>`, for example `1=janus_mace`.
    #[arg(long)]
    runtime_stage_backend: Vec<String>,
    /// Python executable used for the Janus adapter.
    #[arg(long)]
    python_bin: Option<PathBuf>,
    /// Python adapter script used for the Janus/MACE backend.
    #[arg(long)]
    janus_adapter_script: Option<PathBuf>,
    /// Janus architecture, for example `mace_mp`.
    #[arg(long, default_value = "mace_mp")]
    janus_arch: String,
    /// Janus model card or local model path.
    #[arg(long, default_value = "small")]
    janus_model: String,
    /// Janus device such as `cpu`, `cuda`, or `mps`.
    #[arg(long, default_value = "cpu")]
    janus_device: String,
    /// Janus calculator dtype.
    #[arg(long, default_value = "float64")]
    janus_dtype: String,
    /// Janus evaluation mode.
    #[arg(long, default_value = "single-point")]
    janus_mode: DriverJanusMode,
    /// Janus local optimization force threshold.
    #[arg(long, default_value_t = 0.1)]
    janus_fmax: f64,
    /// Janus local optimization step limit.
    #[arg(long, default_value_t = 1000)]
    janus_steps: usize,
    /// Janus local optimization algorithm.
    #[arg(long, default_value = "abc-fire")]
    janus_optimizer: DriverJanusOptimizer,
}

#[derive(Debug, Parser)]
struct RunHybridGaProductionArgs {
    /// Base candidate JSON used as the seed structure for the staged GA phase.
    #[arg(long)]
    base_candidate_json: Option<PathBuf>,
    /// Base workdir used for the hybrid workflow. GA and production subdirectories are created within it.
    #[arg(long)]
    workdir: PathBuf,
    /// Top-level tracked run directory. GA, production, and emulate artifacts are stored under this root.
    #[arg(long)]
    run_dir: PathBuf,
    /// Directory containing Scott-native staged inputs such as `run.job`, `Master.gin`, and optional `atoms.in`.
    #[arg(long)]
    scott_input_dir: Option<PathBuf>,
    /// Optional system label written into the hybrid artifacts.
    #[arg(long, default_value = "unknown-system")]
    system: String,
    /// Number of GA generations.
    #[arg(long, default_value_t = 10)]
    ga_generations: usize,
    /// Population size.
    #[arg(long, default_value_t = 10)]
    population: usize,
    /// Maximum number of production inputs promoted into the production stage.
    #[arg(long, default_value_t = 5)]
    max_production_seeds: usize,
    /// Downstream production seed-selection policy.
    #[arg(long, value_enum, default_value = "top-ranked")]
    seed_selection_mode: HybridSeedSelectionCli,
    /// Candidate variant forwarded from GA into production.
    #[arg(long, value_enum, default_value = "relaxed-result")]
    seed_candidate_mode: HybridSeedCandidateCli,
    /// Scientific hybrid mode used between advisory parent selection and production intake.
    #[arg(long, value_enum, default_value = "ga-downstream-selection")]
    hybrid_scientific_mode: HybridScientificModeCli,
    /// Maximum fixed-parent crossover attempts when `--hybrid-scientific-mode enforced-crossover`.
    #[arg(long, default_value_t = 12)]
    hybrid_crossover_attempts: usize,
    /// Require converged GA members before they are eligible for production.
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    require_converged_ga_seeds: bool,
    /// Optional random seed for deterministic controller behavior.
    #[arg(long)]
    seed: Option<u64>,
    /// Search temperature reserved for later acceptance/replacement parity work.
    #[arg(long, default_value_t = 10.0)]
    temperature: f64,
    /// Geometric perturbation size used by the Rust controller.
    #[arg(long, default_value_t = 0.1)]
    step_size: f64,
    /// Optional timeout in seconds for staged backend requests.
    #[arg(long)]
    timeout_secs: Option<u64>,
    /// Path to the standalone GULP executable used by the staged runtime when `gulp` is routed.
    #[arg(long)]
    executable: Option<PathBuf>,
    /// Path to the Master.gin template containing the KLMC3-RS injection marker.
    #[arg(long)]
    master_gin_template: Option<PathBuf>,
    /// Path to the SCOTT run.job template used to build the staged evaluator and production procedures.
    #[arg(long)]
    run_job_template: Option<PathBuf>,
    /// Optional path to the SCOTT atoms.in template used for Scott-shaped staged GULP runs.
    #[arg(long)]
    atoms_in_template: Option<PathBuf>,
    /// Default runtime backend mode for staged evaluation.
    #[arg(long, default_value = "gulp")]
    runtime_default_backend: String,
    /// Optional per-stage backend override in the form `<stage>=<backend>`, for example `1=janus_mace`.
    #[arg(long)]
    runtime_stage_backend: Vec<String>,
    /// Python executable used for the Janus adapter.
    #[arg(long)]
    python_bin: Option<PathBuf>,
    /// Python adapter script used for the Janus/MACE backend.
    #[arg(long)]
    janus_adapter_script: Option<PathBuf>,
    /// Janus architecture, for example `mace_mp`.
    #[arg(long, default_value = "mace_mp")]
    janus_arch: String,
    /// Janus model card or local model path.
    #[arg(long, default_value = "small")]
    janus_model: String,
    /// Janus device such as `cpu`, `cuda`, or `mps`.
    #[arg(long, default_value = "cpu")]
    janus_device: String,
    /// Janus calculator dtype.
    #[arg(long, default_value = "float64")]
    janus_dtype: String,
    /// Janus evaluation mode used when routing includes `janus_mace`.
    #[arg(long, default_value = "single-point")]
    janus_mode: DriverJanusMode,
    /// Janus local optimization force threshold.
    #[arg(long, default_value_t = 0.1)]
    janus_fmax: f64,
    /// Janus local optimization step limit.
    #[arg(long, default_value_t = 1000)]
    janus_steps: usize,
    /// Janus local optimization algorithm.
    #[arg(long, default_value = "abc-fire")]
    janus_optimizer: DriverJanusOptimizer,
    /// Enable exact Dreadnaut hashkeys for duplicate classification.
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    use_dreadnaut_keys: bool,
    /// Hashkey radius mode, matching native SCOTT conventions.
    #[arg(long, default_value = "IR")]
    hashkey_radius: String,
    /// Extra hashkey radius constant, matching native SCOTT conventions.
    #[arg(long, default_value_t = 0.4)]
    hashkey_radius_const: f64,
    /// PMOI duplicate threshold. Lower is stricter, higher is more permissive.
    #[arg(long, default_value_t = 0.02)]
    pmoi_tolerance: f64,
    /// Enable PMOI as a duplicate fallback after exact native hashkey comparison.
    #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
    enable_pmoi: bool,
    /// Stable emulate campaign identifier used when `--seed-selection-mode acquisition-guided`.
    #[arg(long)]
    emulate_campaign_id: Option<String>,
    /// Stable emulate branch identifier used when `--seed-selection-mode acquisition-guided`.
    #[arg(long, default_value = "branch-hybrid-ga-1")]
    emulate_branch_id: String,
    /// Learning objective requested from the surrogate layer.
    #[arg(long, value_enum, default_value = "reduce-uncertainty")]
    emulate_objective: EmulateLearningObjectiveCli,
    /// Fidelity label attached to the hybrid training and candidate rows.
    #[arg(long, value_enum, default_value = "gulp-reference")]
    emulate_fidelity: EmulateFidelityCli,
    /// Feature projector family used to build emulator input rows.
    #[arg(long, value_enum, default_value = "pair-distance-signature")]
    emulate_feature_projector: EmulateFeatureProjectorCli,
    /// Target name emitted into the surrogate contract.
    #[arg(long, default_value = "energy")]
    emulate_target_name: String,
    /// Optional target unit emitted into the surrogate contract.
    #[arg(long, default_value = "eV")]
    emulate_target_unit: String,
    /// Surrogate model family.
    #[arg(long, value_enum, default_value = "whitened-svgp")]
    emulate_model_variant: SurrogateModelVariantCli,
    /// Surrogate optimization direction.
    #[arg(long, value_enum, default_value = "minimize")]
    emulate_surrogate_objective: SurrogateObjectiveCli,
    /// Number of training epochs.
    #[arg(long, default_value_t = 120)]
    emulate_epochs: usize,
    /// Number of inducing points.
    #[arg(long, default_value_t = 24)]
    emulate_num_inducing: usize,
    /// Mini-batch size.
    #[arg(long, default_value_t = 16)]
    emulate_batch_size: usize,
    /// Learning rate.
    #[arg(long, default_value_t = 0.05)]
    emulate_lr: f64,
    /// Bootstrap ensemble count.
    #[arg(long, default_value_t = 1)]
    emulate_n_bootstraps: usize,
    /// Random seed for the surrogate runtime.
    #[arg(long, default_value_t = 42)]
    emulate_random_seed: u64,
    /// Surrogate runtime log level.
    #[arg(long, default_value = "warning")]
    emulate_log_level: String,
    /// Optional uv binary path.
    #[arg(long)]
    emulate_uv_bin: Option<PathBuf>,
    /// Optional override for the isolated patina-emulate Python project directory.
    #[arg(long)]
    emulate_project_dir: Option<PathBuf>,
    /// uv environment name under the workspace `venvs/` directory.
    #[arg(long, default_value = "autoemulate")]
    emulate_environment: String,
    /// Optional surrogate runtime timeout in seconds.
    #[arg(long, default_value_t = 300)]
    emulate_timeout_secs: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
enum RunnerMode {
    Transient,
    Persistent,
}

#[derive(Debug, Parser)]
struct EvaluateBackendBatchArgs {
    /// Backend to use for this batch evaluation.
    #[arg(long, value_enum)]
    backend: EvalBackendKind,
    /// Candidate JSON files matching `patina_types::Candidate`.
    #[arg(long, required = true, num_args = 1..)]
    candidate_json: Vec<PathBuf>,
    /// Base workdir used for this batch evaluation.
    #[arg(long)]
    workdir: PathBuf,
    /// Runner execution mode. Defaults to the current transient sandbox path.
    #[arg(long, value_enum, default_value = "transient")]
    runner_mode: RunnerMode,
    /// Number of worker slots used for the batch evaluation.
    #[arg(long)]
    workers: Option<usize>,
    /// Keep worker directories after evaluation for debugging.
    #[arg(long, default_value_t = false)]
    keep_dirs: bool,
    /// Optional timeout in seconds.
    #[arg(long)]
    timeout_secs: Option<u64>,
    /// Path to the GULP or SCOTT executable, depending on backend.
    #[arg(long)]
    executable: Option<PathBuf>,
    /// Path to the Master.gin template containing the KLMC3-RS injection marker.
    #[arg(long)]
    master_gin_template: Option<PathBuf>,
    /// Path to the SCOTT run.job template.
    #[arg(long)]
    run_job_template: Option<PathBuf>,
    /// Path to the SCOTT atoms.in template.
    #[arg(long)]
    atoms_in_template: Option<PathBuf>,
    /// Optional path to the SCOTT jobs file template.
    #[arg(long)]
    jobs_template: Option<PathBuf>,
    /// Python executable used for the Janus adapter.
    #[arg(long)]
    python_bin: Option<PathBuf>,
    /// Python adapter script used for the Janus/MACE backend.
    #[arg(long)]
    janus_adapter_script: Option<PathBuf>,
    /// Janus architecture, for example `mace_mp`.
    #[arg(long, default_value = "mace_mp")]
    janus_arch: String,
    /// Janus model card or local model path.
    #[arg(long, default_value = "small")]
    janus_model: String,
    /// Janus device such as `cpu`, `cuda`, or `mps`.
    #[arg(long, default_value = "cpu")]
    janus_device: String,
    /// Janus calculator dtype.
    #[arg(long, default_value = "float64")]
    janus_dtype: String,
    /// Janus evaluation mode.
    #[arg(long, default_value = "single-point")]
    janus_mode: DriverJanusMode,
    /// Janus local optimization force threshold.
    #[arg(long, default_value_t = 0.1)]
    janus_fmax: f64,
    /// Janus local optimization step limit.
    #[arg(long, default_value_t = 1000)]
    janus_steps: usize,
    /// Janus local optimization algorithm.
    #[arg(long, default_value = "abc-fire")]
    janus_optimizer: DriverJanusOptimizer,
}

#[derive(Debug, Parser)]
struct CompareBackendBatchArgs {
    /// Backend to use for this batch comparison.
    #[arg(long, value_enum)]
    backend: EvalBackendKind,
    /// Candidate JSON files matching `patina_types::Candidate`.
    #[arg(long, required = true, num_args = 1..)]
    candidate_json: Vec<PathBuf>,
    /// Base workdir used for this batch evaluation comparison.
    #[arg(long)]
    workdir: PathBuf,
    /// Number of worker slots used for both batch evaluations.
    #[arg(long)]
    workers: Option<usize>,
    /// Keep worker directories after evaluation for debugging.
    #[arg(long, default_value_t = false)]
    keep_dirs: bool,
    /// Optional timeout in seconds.
    #[arg(long)]
    timeout_secs: Option<u64>,
    /// Path to the GULP or SCOTT executable, depending on backend.
    #[arg(long)]
    executable: Option<PathBuf>,
    /// Path to the Master.gin template containing the KLMC3-RS injection marker.
    #[arg(long)]
    master_gin_template: Option<PathBuf>,
    /// Path to the SCOTT run.job template.
    #[arg(long)]
    run_job_template: Option<PathBuf>,
    /// Path to the SCOTT atoms.in template.
    #[arg(long)]
    atoms_in_template: Option<PathBuf>,
    /// Optional path to the SCOTT jobs file template.
    #[arg(long)]
    jobs_template: Option<PathBuf>,
    /// Python executable used for the Janus adapter.
    #[arg(long)]
    python_bin: Option<PathBuf>,
    /// Python adapter script used for the Janus/MACE backend.
    #[arg(long)]
    janus_adapter_script: Option<PathBuf>,
    /// Janus architecture, for example `mace_mp`.
    #[arg(long, default_value = "mace_mp")]
    janus_arch: String,
    /// Janus model card or local model path.
    #[arg(long, default_value = "small")]
    janus_model: String,
    /// Janus device such as `cpu`, `cuda`, or `mps`.
    #[arg(long, default_value = "cpu")]
    janus_device: String,
    /// Janus calculator dtype.
    #[arg(long, default_value = "float64")]
    janus_dtype: String,
    /// Janus evaluation mode.
    #[arg(long, default_value = "single-point")]
    janus_mode: DriverJanusMode,
    /// Janus local optimization force threshold.
    #[arg(long, default_value_t = 0.1)]
    janus_fmax: f64,
    /// Janus local optimization step limit.
    #[arg(long, default_value_t = 1000)]
    janus_steps: usize,
    /// Janus local optimization algorithm.
    #[arg(long, default_value = "abc-fire")]
    janus_optimizer: DriverJanusOptimizer,
}

#[derive(Debug, Parser)]
struct CompareBackendGenerationsArgs {
    /// Backend to use for this generation comparison.
    #[arg(long, value_enum)]
    backend: EvalBackendKind,
    /// Candidate JSON files matching `patina_types::Candidate`.
    #[arg(long, required = true, num_args = 1..)]
    candidate_json: Vec<PathBuf>,
    /// Number of sequential generations to run in one process.
    #[arg(long, default_value_t = 3)]
    generations: usize,
    /// Base workdir used for this generation comparison.
    #[arg(long)]
    workdir: PathBuf,
    /// Number of worker slots used for all generations.
    #[arg(long)]
    workers: Option<usize>,
    /// Keep worker directories after evaluation for debugging.
    #[arg(long, default_value_t = false)]
    keep_dirs: bool,
    /// Optional timeout in seconds.
    #[arg(long)]
    timeout_secs: Option<u64>,
    /// Path to the GULP or SCOTT executable, depending on backend.
    #[arg(long)]
    executable: Option<PathBuf>,
    /// Path to the Master.gin template containing the KLMC3-RS injection marker.
    #[arg(long)]
    master_gin_template: Option<PathBuf>,
    /// Path to the SCOTT run.job template.
    #[arg(long)]
    run_job_template: Option<PathBuf>,
    /// Path to the SCOTT atoms.in template.
    #[arg(long)]
    atoms_in_template: Option<PathBuf>,
    /// Optional path to the SCOTT jobs file template.
    #[arg(long)]
    jobs_template: Option<PathBuf>,
    /// Python executable used for the Janus adapter.
    #[arg(long)]
    python_bin: Option<PathBuf>,
    /// Python adapter script used for the Janus/MACE backend.
    #[arg(long)]
    janus_adapter_script: Option<PathBuf>,
    /// Janus architecture, for example `mace_mp`.
    #[arg(long, default_value = "mace_mp")]
    janus_arch: String,
    /// Janus model card or local model path.
    #[arg(long, default_value = "small")]
    janus_model: String,
    /// Janus device such as `cpu`, `cuda`, or `mps`.
    #[arg(long, default_value = "cpu")]
    janus_device: String,
    /// Janus calculator dtype.
    #[arg(long, default_value = "float64")]
    janus_dtype: String,
    /// Janus evaluation mode.
    #[arg(long, default_value = "single-point")]
    janus_mode: DriverJanusMode,
    /// Janus local optimization force threshold.
    #[arg(long, default_value_t = 0.1)]
    janus_fmax: f64,
    /// Janus local optimization step limit.
    #[arg(long, default_value_t = 1000)]
    janus_steps: usize,
    /// Janus local optimization algorithm.
    #[arg(long, default_value = "abc-fire")]
    janus_optimizer: DriverJanusOptimizer,
}

#[derive(Debug, Parser)]
struct RunBackendCampaignArgs {
    /// Backend to use for this tracked evaluator campaign.
    #[arg(long, value_enum)]
    backend: EvalBackendKind,
    /// Candidate JSON files matching `patina_types::Candidate`.
    #[arg(long, required = true, num_args = 1..)]
    candidate_json: Vec<PathBuf>,
    /// Number of sequential static-replay rounds to run in one process.
    /// This command does not evolve or propagate candidates between generations.
    #[arg(long, default_value_t = 20)]
    generations: usize,
    /// Base workdir used for this campaign.
    #[arg(long)]
    workdir: PathBuf,
    /// Tracked run directory under `runs/active/` or `runs/archive/`.
    #[arg(long)]
    run_dir: PathBuf,
    /// User-facing system label stored in the campaign manifest.
    #[arg(long, default_value = "unknown-system")]
    system: String,
    /// Worker execution mode for the evaluator campaign.
    #[arg(long, value_enum, default_value = "persistent")]
    runner_mode: RunnerMode,
    /// Number of worker slots used for all generations.
    #[arg(long)]
    workers: Option<usize>,
    /// Keep worker directories after evaluation for debugging.
    #[arg(long, default_value_t = false)]
    keep_dirs: bool,
    /// Optional timeout in seconds.
    #[arg(long)]
    timeout_secs: Option<u64>,
    /// Optional atoms.in template used for topology summary radii.
    #[arg(long)]
    atoms_in_template: Option<PathBuf>,
    /// Path to the GULP or SCOTT executable, depending on backend.
    #[arg(long)]
    executable: Option<PathBuf>,
    /// Path to the Master.gin template containing the KLMC3-RS injection marker.
    #[arg(long)]
    master_gin_template: Option<PathBuf>,
    /// Path to the SCOTT run.job template.
    #[arg(long)]
    run_job_template: Option<PathBuf>,
    /// Optional path to the SCOTT jobs file template.
    #[arg(long)]
    jobs_template: Option<PathBuf>,
    /// Python executable used for the Janus adapter.
    #[arg(long)]
    python_bin: Option<PathBuf>,
    /// Python adapter script used for the Janus/MACE backend.
    #[arg(long)]
    janus_adapter_script: Option<PathBuf>,
    /// Janus architecture, for example `mace_mp`.
    #[arg(long, default_value = "mace_mp")]
    janus_arch: String,
    /// Janus model card or local model path.
    #[arg(long, default_value = "small")]
    janus_model: String,
    /// Janus device such as `cpu`, `cuda`, or `mps`.
    #[arg(long, default_value = "cpu")]
    janus_device: String,
    /// Janus calculator dtype.
    #[arg(long, default_value = "float64")]
    janus_dtype: String,
    /// Janus evaluation mode.
    #[arg(long, default_value = "local-opt")]
    janus_mode: DriverJanusMode,
    /// Janus local optimization force threshold.
    #[arg(long, default_value_t = 0.1)]
    janus_fmax: f64,
    /// Janus local optimization step limit.
    #[arg(long, default_value_t = 1000)]
    janus_steps: usize,
    /// Janus local optimization algorithm.
    #[arg(long, default_value = "abc-fire")]
    janus_optimizer: DriverJanusOptimizer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
enum DriverJanusMode {
    SinglePoint,
    LocalOpt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
enum DriverJanusOptimizer {
    Lbfgs,
    Fire,
    Fire2,
    AbcFire,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
enum RustJanusDuplicatePolicyCliMode {
    ExternalNativeHashkey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
enum EmulateLearningObjectiveCli {
    ScoreCandidates,
    ReduceUncertainty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
enum EmulateFidelityCli {
    DescriptorOnly,
    GulpReference,
    JanusMaceLow,
    JanusMaceHigh,
    ReferenceDft,
    MixedEvidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
enum EmulateFeatureProjectorCli {
    SimpleStructureStatistics,
    PairDistanceSignature,
    DscribeSoap,
    FeatomicSoap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
enum SurrogateModelVariantCli {
    #[serde(rename = "mean-field-svgp")]
    #[value(name = "mean-field-svgp")]
    MeanField,
    #[serde(rename = "unwhitened-svgp")]
    #[value(name = "unwhitened-svgp")]
    Unwhitened,
    #[serde(rename = "whitened-svgp")]
    #[value(name = "whitened-svgp")]
    Whitened,
    #[serde(rename = "multitask-mean-field-svgp")]
    #[value(name = "multitask-mean-field-svgp")]
    MultitaskMeanField,
    #[serde(rename = "multitask-unwhitened-svgp")]
    #[value(name = "multitask-unwhitened-svgp")]
    MultitaskUnwhitened,
    #[serde(rename = "multitask-whitened-svgp")]
    #[value(name = "multitask-whitened-svgp")]
    MultitaskWhitened,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
enum SurrogateObjectiveCli {
    Minimize,
    Maximize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
enum HybridSeedSelectionCli {
    TopRanked,
    AcquisitionGuided,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
enum HybridSeedCandidateCli {
    RelaxedResult,
    SourceCandidate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
enum HybridScientificModeCli {
    GaDownstreamSelection,
    EnforcedCrossover,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
enum DriverBhMethodCli {
    Relax,
    Fixed,
    Oscillate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
enum DriverBhAcceptCli {
    Metropolis,
    Quench,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
enum DriverScanSurfaceMoveCli {
    RandomizedLocation,
    TranslateCluster,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
enum DriverScanSurfaceAcceptCli {
    Metropolis,
    DownhillOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
enum DriverSolidSolutionsMoveCli {
    MixSolution,
    RandomizeSolution,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
enum DriverSolidSolutionsAcceptCli {
    Metropolis,
    DownhillOnly,
    RecordAll,
}

impl DriverJanusMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::SinglePoint => "single-point",
            Self::LocalOpt => "local-opt",
        }
    }
}

impl From<EmulateLearningObjectiveCli> for patina_emulate::LearningObjective {
    fn from(value: EmulateLearningObjectiveCli) -> Self {
        match value {
            EmulateLearningObjectiveCli::ScoreCandidates => Self::ScoreCandidates,
            EmulateLearningObjectiveCli::ReduceUncertainty => Self::ReduceUncertainty,
        }
    }
}

impl From<EmulateFidelityCli> for patina_emulate::FidelityClass {
    fn from(value: EmulateFidelityCli) -> Self {
        match value {
            EmulateFidelityCli::DescriptorOnly => Self::DescriptorOnly,
            EmulateFidelityCli::GulpReference => Self::GulpReference,
            EmulateFidelityCli::JanusMaceLow => Self::JanusMaceLow,
            EmulateFidelityCli::JanusMaceHigh => Self::JanusMaceHigh,
            EmulateFidelityCli::ReferenceDft => Self::ReferenceDft,
            EmulateFidelityCli::MixedEvidence => Self::MixedEvidence,
        }
    }
}

impl From<EmulateFeatureProjectorCli> for patina_emulate::FeatureProjectorKind {
    fn from(value: EmulateFeatureProjectorCli) -> Self {
        match value {
            EmulateFeatureProjectorCli::SimpleStructureStatistics => {
                Self::SimpleStructureStatistics
            }
            EmulateFeatureProjectorCli::PairDistanceSignature => Self::PairDistanceSignature,
            EmulateFeatureProjectorCli::DscribeSoap => Self::DscribeSoap,
            EmulateFeatureProjectorCli::FeatomicSoap => Self::FeatomicSoap,
        }
    }
}

impl From<SurrogateModelVariantCli> for patina_emulate::SurrogateModelVariant {
    fn from(value: SurrogateModelVariantCli) -> Self {
        match value {
            SurrogateModelVariantCli::MeanField => Self::MeanFieldSvgp,
            SurrogateModelVariantCli::Unwhitened => Self::UnwhitenedSvgp,
            SurrogateModelVariantCli::Whitened => Self::WhitenedSvgp,
            SurrogateModelVariantCli::MultitaskMeanField => Self::MultitaskMeanFieldSvgp,
            SurrogateModelVariantCli::MultitaskUnwhitened => Self::MultitaskUnwhitenedSvgp,
            SurrogateModelVariantCli::MultitaskWhitened => Self::MultitaskWhitenedSvgp,
        }
    }
}

impl From<SurrogateObjectiveCli> for patina_emulate::SurrogateObjective {
    fn from(value: SurrogateObjectiveCli) -> Self {
        match value {
            SurrogateObjectiveCli::Minimize => Self::Minimize,
            SurrogateObjectiveCli::Maximize => Self::Maximize,
        }
    }
}

impl From<HybridSeedSelectionCli> for application::hybrid_ga_production::HybridSeedSelectionMode {
    fn from(value: HybridSeedSelectionCli) -> Self {
        match value {
            HybridSeedSelectionCli::TopRanked => Self::TopRanked,
            HybridSeedSelectionCli::AcquisitionGuided => Self::AcquisitionGuided,
        }
    }
}

impl From<HybridSeedCandidateCli> for application::hybrid_ga_production::HybridSeedCandidateMode {
    fn from(value: HybridSeedCandidateCli) -> Self {
        match value {
            HybridSeedCandidateCli::RelaxedResult => Self::RelaxedResult,
            HybridSeedCandidateCli::SourceCandidate => Self::SourceCandidate,
        }
    }
}

impl From<HybridScientificModeCli> for patina_types::HybridCrossoverScientificMode {
    fn from(value: HybridScientificModeCli) -> Self {
        match value {
            HybridScientificModeCli::GaDownstreamSelection => Self::GaDownstreamSelection,
            HybridScientificModeCli::EnforcedCrossover => Self::EnforcedCrossover,
        }
    }
}

impl From<DriverJanusMode> for JanusMode {
    fn from(value: DriverJanusMode) -> Self {
        match value {
            DriverJanusMode::SinglePoint => JanusMode::SinglePoint,
            DriverJanusMode::LocalOpt => JanusMode::LocalOpt,
        }
    }
}

impl From<DriverJanusMode> for application::driver_support::JanusModeSetting {
    fn from(value: DriverJanusMode) -> Self {
        match value {
            DriverJanusMode::SinglePoint => Self::SinglePoint,
            DriverJanusMode::LocalOpt => Self::LocalOpt,
        }
    }
}

impl From<application::driver_support::JanusModeSetting> for DriverJanusMode {
    fn from(value: application::driver_support::JanusModeSetting) -> Self {
        match value {
            application::driver_support::JanusModeSetting::SinglePoint => Self::SinglePoint,
            application::driver_support::JanusModeSetting::LocalOpt => Self::LocalOpt,
        }
    }
}

impl DriverJanusOptimizer {
    fn as_str(self) -> &'static str {
        match self {
            Self::Lbfgs => "lbfgs",
            Self::Fire => "fire",
            Self::Fire2 => "fire2",
            Self::AbcFire => "abc-fire",
        }
    }
}

impl From<DriverJanusOptimizer> for JanusOptimizer {
    fn from(value: DriverJanusOptimizer) -> Self {
        match value {
            DriverJanusOptimizer::Lbfgs => JanusOptimizer::Lbfgs,
            DriverJanusOptimizer::Fire => JanusOptimizer::Fire,
            DriverJanusOptimizer::Fire2 => JanusOptimizer::Fire2,
            DriverJanusOptimizer::AbcFire => JanusOptimizer::AbcFire,
        }
    }
}

impl From<DriverJanusOptimizer> for application::driver_support::JanusOptimizerSetting {
    fn from(value: DriverJanusOptimizer) -> Self {
        match value {
            DriverJanusOptimizer::Lbfgs => Self::Lbfgs,
            DriverJanusOptimizer::Fire => Self::Fire,
            DriverJanusOptimizer::Fire2 => Self::Fire2,
            DriverJanusOptimizer::AbcFire => Self::AbcFire,
        }
    }
}

impl From<application::driver_support::JanusOptimizerSetting> for DriverJanusOptimizer {
    fn from(value: application::driver_support::JanusOptimizerSetting) -> Self {
        match value {
            application::driver_support::JanusOptimizerSetting::Lbfgs => Self::Lbfgs,
            application::driver_support::JanusOptimizerSetting::Fire => Self::Fire,
            application::driver_support::JanusOptimizerSetting::Fire2 => Self::Fire2,
            application::driver_support::JanusOptimizerSetting::AbcFire => Self::AbcFire,
        }
    }
}

impl From<EvalBackendKind> for application::driver_support::EvalBackendKind {
    fn from(value: EvalBackendKind) -> Self {
        match value {
            EvalBackendKind::Scott => Self::Scott,
            EvalBackendKind::Gulp => Self::Gulp,
            EvalBackendKind::JanusMace => Self::JanusMace,
        }
    }
}

impl From<application::driver_support::EvalBackendKind> for EvalBackendKind {
    fn from(value: application::driver_support::EvalBackendKind) -> Self {
        match value {
            application::driver_support::EvalBackendKind::Scott => Self::Scott,
            application::driver_support::EvalBackendKind::Gulp => Self::Gulp,
            application::driver_support::EvalBackendKind::JanusMace => Self::JanusMace,
        }
    }
}

impl From<DriverScanSurfaceMoveCli> for application::ports::ScanSurfaceMoveMode {
    fn from(value: DriverScanSurfaceMoveCli) -> Self {
        match value {
            DriverScanSurfaceMoveCli::RandomizedLocation => Self::RandomizedLocation,
            DriverScanSurfaceMoveCli::TranslateCluster => Self::TranslateCluster,
        }
    }
}

impl From<DriverScanSurfaceAcceptCli> for application::ports::ScanSurfaceAcceptanceMode {
    fn from(value: DriverScanSurfaceAcceptCli) -> Self {
        match value {
            DriverScanSurfaceAcceptCli::Metropolis => Self::Metropolis,
            DriverScanSurfaceAcceptCli::DownhillOnly => Self::DownhillOnly,
        }
    }
}

impl From<DriverSolidSolutionsMoveCli> for application::ports::SolidSolutionsMoveMode {
    fn from(value: DriverSolidSolutionsMoveCli) -> Self {
        match value {
            DriverSolidSolutionsMoveCli::MixSolution => Self::MixSolution,
            DriverSolidSolutionsMoveCli::RandomizeSolution => Self::RandomizeSolution,
        }
    }
}

impl From<DriverSolidSolutionsAcceptCli> for application::ports::SolidSolutionsAcceptanceMode {
    fn from(value: DriverSolidSolutionsAcceptCli) -> Self {
        match value {
            DriverSolidSolutionsAcceptCli::Metropolis => Self::Metropolis,
            DriverSolidSolutionsAcceptCli::DownhillOnly => Self::DownhillOnly,
            DriverSolidSolutionsAcceptCli::RecordAll => Self::RecordAll,
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Status => {
            println!("{}", application::status::render_status_report());
        }
        Command::Workflow(args) => handle_workflow_command(args)?,
        Command::RunScottSearch(args) => run_scott_search(args)?,
        Command::RunGa(args) => handle_ga_mode_command(args.mode)?,
        Command::RunRustJanusSearch(args) => run_rust_janus_search(*args)?,
        Command::RunScottStagedGa(args) => run_scott_staged_ga(*args)?,
        Command::EvaluateScott(args) => evaluate_scott(args)?,
        Command::EvaluateBackend(args) => evaluate_backend(args)?,
        Command::EvaluateScottStaged(args) => evaluate_scott_staged(args)?,
        Command::RunScottProduction(args) => run_scott_production(args)?,
        Command::RunHybridGaProduction(args) => run_hybrid_ga_production(*args)?,
        Command::RunBasinHopping(args) => run_basin_hopping(args)?,
        Command::RunEnergyLid(args) => run_energy_lid(args)?,
        Command::RunScanSurface(args) => run_scan_surface(args)?,
        Command::RunSolidSolutions(args) => run_solid_solutions(args)?,
        Command::ScoreGaEmulate(args) => run_score_ga_emulate(args)?,
        Command::RunSimulatedAnnealing(args) => run_simulated_annealing(args)?,
        Command::PerturbCluster(args) => perturb_cluster(args)?,
        Command::GenerateSurface(args) => generate_surface(args)?,
        Command::RunSurfaceReconstruction(args) => run_surface_reconstruction(args)?,
        Command::AnalyzeSurfacePolarity(args) => analyze_surface_polarity(args)?,
        Command::AnalyzeFrameworkSymmetry(args) => analyze_framework_symmetry(args)?,
        Command::NormalizeFramework(args) => normalize_framework(args)?,
        Command::RunFrameworkGcmc(args) => run_framework_gcmc(args)?,
        Command::Structure(args) => run_structure_command(args)?,
        Command::ConstructStkZeroD(args) => construct_stk_zero_d(args)?,
        Command::EvaluateBackendBatch(args) => evaluate_backend_batch(args)?,
        Command::CompareBackendBatch(args) => compare_backend_batch(args)?,
        Command::CompareBackendGenerations(args) => compare_backend_generations(args)?,
        Command::RunBackendCampaign(args) => run_backend_campaign(args)?,
        Command::CompareDuplicateParity(args) => {
            application::duplicate_parity::compare_duplicate_parity(
                application::duplicate_parity::CompareDuplicateParityRequest {
                    run_dir: args.run_dir,
                    pmoi_tolerance: args.pmoi_tolerance,
                },
            )?
        }
        Command::CompareDuplicateEdgeCases(args) => {
            application::duplicate_parity::compare_duplicate_edge_cases(
                application::duplicate_parity::CompareDuplicateEdgeCasesRequest {
                    output_dir: args.output_dir,
                    atoms_in_template: args.atoms_in_template,
                    use_dreadnaut_keys: args.use_dreadnaut_keys,
                    hashkey_radius: args.hashkey_radius,
                    hashkey_radius_const: args.hashkey_radius_const,
                    pmoi_tolerance: args.pmoi_tolerance,
                },
            )?
        }
    }

    Ok(())
}

fn render_run_ga_architecture_help() -> &'static str {
    r#"run-ga architecture

Shared GA layers
- Domain controller: ScottParityGeneticAlgorithm
- Application workflow: GaWorkflowService
- Shared semantics: selection, spawning, replacement, duplicate handling, repopulation

Adapter modes
- persistent-daemon
  evaluation port: PersistentPoolGaEvaluator
  runtime path: PersistentWorkerPool -> PersistentJanusMaceBackend
  parallelism control: --workers
  connection semantics: long-lived daemon workers with reused sandboxes

- scott-monolithic
  evaluation port: StagedScottGaEvaluator
  runtime path: run_local_procedure -> RoutedBackendStageExecutor -> GulpBackend/JanusMaceBackend
  parallelism control: RAYON_NUM_THREADS
  connection semantics: monolithic Scott procedure with per-stage backend execution

Diagnostic rule
- If a GA population bug appears in both modes, suspect the shared controller/workflow first.
- If it appears in one mode only, suspect that mode's evaluation adapter or backend path.
"#
}

fn ensure_runner_mode_supported(backend: EvalBackendKind, runner_mode: RunnerMode) -> Result<()> {
    if runner_mode == RunnerMode::Persistent && !backend.supports_persistent_parallel_workers() {
        return Err(anyhow!(
            "runner mode `persistent` is reserved for the Janus/MACE worker-pool pathway in the new Rust-owned branch; backend `{}` should use transient scheduling here",
            backend.engine_label()
        ));
    }
    Ok(())
}

fn load_scott_search_config(path: &Path) -> Result<ScottSearchConfigFile> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read SCOTT search config `{}`", path.display()))?;
    toml::from_str(&raw)
        .with_context(|| format!("failed to parse SCOTT search config `{}`", path.display()))
}

fn resolve_scott_search_run(
    args: RunScottSearchArgs,
) -> Result<(ResolvedScottSearchRun, Option<ScottSearchConfigFile>)> {
    let file_cfg = match args.config.as_ref() {
        Some(path) => Some(load_scott_search_config(path)?),
        None => None,
    };
    let cfg = file_cfg.as_ref();
    let mut cli_overrides = Vec::new();

    let job_type = resolve_legacy_scott_job_type(&args, cfg, &mut cli_overrides)?;
    let trace_mode = SearchMode::from_legacy_job_type(job_type);
    let scott_bin = pick_required(
        args.scott_bin.clone(),
        cfg.and_then(|value| value.scott_bin.clone()),
        "scott_bin",
        &mut cli_overrides,
    )?;
    let data_dir = pick_required(
        args.data_dir.clone(),
        cfg.and_then(|value| value.data_dir.clone()),
        "data_dir",
        &mut cli_overrides,
    )?;
    let workdir = pick_required(
        args.workdir.clone(),
        cfg.and_then(|value| value.workdir.clone()),
        "workdir",
        &mut cli_overrides,
    )?;
    let run_dir = pick_required(
        args.run_dir.clone(),
        cfg.and_then(|value| value.run_dir.clone()),
        "run_dir",
        &mut cli_overrides,
    )?;
    let system = pick_optional(
        args.system.clone(),
        cfg.and_then(|value| value.system.clone()),
        "system",
        &mut cli_overrides,
    )
    .unwrap_or_else(|| "unknown-system".to_string());
    let timeout_secs = pick_optional(
        args.timeout_secs,
        cfg.and_then(|value| value.timeout_secs),
        "timeout_secs",
        &mut cli_overrides,
    );

    let shared_cfg = cfg.map(|value| value.shared.clone()).unwrap_or_default();
    let backend_cfg = cfg.map(|value| value.backend.clone()).unwrap_or_default();
    let runtime_routing_cfg = cfg
        .map(|value| value.runtime_routing.clone())
        .unwrap_or_default();
    let bh_cfg = cfg.map(|value| value.bh.clone()).unwrap_or_default();
    let ga_cfg = cfg.map(|value| value.ga.clone()).unwrap_or_default();

    Ok((
        ResolvedScottSearchRun {
            job_type,
            trace_mode,
            scott_bin,
            data_dir,
            workdir,
            run_dir,
            system,
            timeout_secs,
            shared: ScottSharedSettings {
                population: pick_optional(
                    args.population,
                    shared_cfg.population,
                    "population",
                    &mut cli_overrides,
                ),
                seed: pick_optional(args.seed, shared_cfg.seed, "seed", &mut cli_overrides),
                temperature: pick_optional(
                    args.temperature,
                    shared_cfg.temperature,
                    "temperature",
                    &mut cli_overrides,
                ),
                step_size: pick_optional(
                    args.step_size,
                    shared_cfg.step_size,
                    "step_size",
                    &mut cli_overrides,
                ),
                boundary: pick_optional(
                    args.boundary,
                    shared_cfg.boundary,
                    "boundary",
                    &mut cli_overrides,
                ),
                collapse: pick_optional(
                    args.collapse,
                    shared_cfg.collapse,
                    "collapse",
                    &mut cli_overrides,
                ),
                fragment: pick_optional(
                    args.fragment,
                    shared_cfg.fragment,
                    "fragment",
                    &mut cli_overrides,
                ),
                dspecies: pick_optional(
                    args.dspecies,
                    shared_cfg.dspecies,
                    "dspecies",
                    &mut cli_overrides,
                ),
                output_level: pick_optional(
                    args.output_level,
                    shared_cfg.output_level,
                    "output_level",
                    &mut cli_overrides,
                ),
                use_top_analysis: pick_optional(
                    args.use_top_analysis,
                    shared_cfg.use_top_analysis,
                    "use_top_analysis",
                    &mut cli_overrides,
                ),
                use_dreadnaut_keys: pick_optional(
                    args.use_dreadnaut_keys,
                    shared_cfg.use_dreadnaut_keys,
                    "use_dreadnaut_keys",
                    &mut cli_overrides,
                ),
                hashkey_radius: pick_optional(
                    args.hashkey_radius,
                    shared_cfg.hashkey_radius,
                    "hashkey_radius",
                    &mut cli_overrides,
                ),
                hashkey_radius_const: pick_optional(
                    args.hashkey_radius_const,
                    shared_cfg.hashkey_radius_const,
                    "hashkey_radius_const",
                    &mut cli_overrides,
                ),
            },
            backend: ScottBackendSettings {
                evaluator_backend: pick_optional(
                    args.evaluator_backend,
                    backend_cfg.evaluator_backend,
                    "backend.evaluator_backend",
                    &mut cli_overrides,
                ),
                atoms_in_template: None,
                janus_python: pick_optional(
                    args.janus_python,
                    backend_cfg.janus_python,
                    "backend.janus_python",
                    &mut cli_overrides,
                ),
                janus_adapter: pick_optional(
                    args.janus_adapter,
                    backend_cfg.janus_adapter,
                    "backend.janus_adapter",
                    &mut cli_overrides,
                ),
                janus_mode: pick_optional(
                    args.janus_mode,
                    backend_cfg.janus_mode,
                    "backend.janus_mode",
                    &mut cli_overrides,
                ),
                janus_arch: pick_optional(
                    args.janus_arch,
                    backend_cfg.janus_arch,
                    "backend.janus_arch",
                    &mut cli_overrides,
                ),
                janus_model: pick_optional(
                    args.janus_model,
                    backend_cfg.janus_model,
                    "backend.janus_model",
                    &mut cli_overrides,
                ),
                janus_device: pick_optional(
                    args.janus_device,
                    backend_cfg.janus_device,
                    "backend.janus_device",
                    &mut cli_overrides,
                ),
                janus_dtype: pick_optional(
                    args.janus_dtype,
                    backend_cfg.janus_dtype,
                    "backend.janus_dtype",
                    &mut cli_overrides,
                ),
                janus_fmax: pick_optional(
                    args.janus_fmax,
                    backend_cfg.janus_fmax,
                    "backend.janus_fmax",
                    &mut cli_overrides,
                ),
                janus_steps: pick_optional(
                    args.janus_steps,
                    backend_cfg.janus_steps,
                    "backend.janus_steps",
                    &mut cli_overrides,
                ),
            },
            runtime_routing: runtime_routing_cfg,
            bh: ScottBhSettings {
                bh_steps: pick_optional(
                    args.bh_steps,
                    bh_cfg.bh_steps,
                    "bh_steps",
                    &mut cli_overrides,
                ),
                ..bh_cfg
            },
            ga: ScottGaSettings {
                ga_generations: pick_optional(
                    args.ga_generations,
                    ga_cfg.ga_generations,
                    "ga_generations",
                    &mut cli_overrides,
                ),
                save_pop_out_format: ga_cfg.save_pop_out_format.clone(),
                ..ga_cfg
            },
            config_path: args.config,
            cli_overrides,
        },
        file_cfg,
    ))
}

fn resolve_legacy_scott_job_type(
    args: &RunScottSearchArgs,
    cfg: Option<&ScottSearchConfigFile>,
    cli_overrides: &mut Vec<String>,
) -> Result<application::legacy_scott::LegacyScottJobType> {
    let cli_job_type = args.job_type.map(Into::into);
    let cfg_job_type = cfg.and_then(|value| value.job_type);
    let cli_mode = args.mode;
    let cfg_mode = cfg.and_then(|value| value.mode);

    if cli_job_type.is_some() {
        cli_overrides.push("job_type".to_string());
    }
    if cli_mode.is_some() {
        cli_overrides.push("mode".to_string());
    }

    ensure_legacy_scott_job_type_consistency("cli", cli_job_type, cli_mode)?;
    ensure_legacy_scott_job_type_consistency("config", cfg_job_type, cfg_mode)?;

    cli_job_type
        .or_else(|| cli_mode.map(SearchMode::into_legacy_job_type))
        .or(cfg_job_type)
        .or_else(|| cfg_mode.map(SearchMode::into_legacy_job_type))
        .ok_or_else(|| {
            anyhow!("missing required SCOTT search setting `job_type` or legacy shorthand `mode`")
        })
}

fn ensure_legacy_scott_job_type_consistency(
    source: &str,
    job_type: Option<application::legacy_scott::LegacyScottJobType>,
    mode: Option<SearchMode>,
) -> Result<()> {
    let Some(mode) = mode else {
        return Ok(());
    };
    let Some(job_type) = job_type else {
        return Ok(());
    };
    let expected = mode.into_legacy_job_type();
    if job_type == expected {
        return Ok(());
    }
    Err(anyhow!(
        "SCOTT search {source} mixes incompatible `job_type={}` with legacy `mode={}`",
        job_type.as_str(),
        mode.as_str(),
    ))
}

fn pick_required<T: Clone>(
    cli_value: Option<T>,
    cfg_value: Option<T>,
    field: &str,
    cli_overrides: &mut Vec<String>,
) -> Result<T> {
    pick_optional(cli_value, cfg_value, field, cli_overrides)
        .ok_or_else(|| anyhow!("missing required SCOTT search setting `{field}`"))
}

fn pick_optional<T: Clone>(
    cli_value: Option<T>,
    cfg_value: Option<T>,
    field: &str,
    cli_overrides: &mut Vec<String>,
) -> Option<T> {
    if cli_value.is_some() {
        cli_overrides.push(field.to_string());
        cli_value
    } else {
        cfg_value
    }
}

fn run_scott_search(args: RunScottSearchArgs) -> Result<()> {
    let (request, file_cfg) = resolve_scott_search_run(args)?;

    reset_scott_search_workdir(&request.workdir)?;
    fs::create_dir_all(&request.workdir).with_context(|| {
        format!(
            "failed to create SCOTT search workdir `{}`",
            request.workdir.display()
        )
    })?;
    let data_dir = request.workdir.join("data");
    fs::create_dir_all(&data_dir).with_context(|| {
        format!(
            "failed to create staged data directory `{}`",
            data_dir.display()
        )
    })?;

    for name in ["atoms.in", "Master.gin", "jobs", "run.job"] {
        let source = request.data_dir.join(name);
        let target = data_dir.join(name);
        fs::copy(&source, &target).with_context(|| {
            format!(
                "failed to copy required SCOTT input `{}` to `{}`",
                source.display(),
                target.display()
            )
        })?;
    }

    let patch_summary =
        patch_scott_run_job(&data_dir.join("run.job"), &request, file_cfg.as_ref())?;

    let mut child = ProcessCommand::new(&request.scott_bin)
        .current_dir(&request.workdir)
        .env("OMPI_MCA_btl", "self")
        .env("OMPI_MCA_btl_base_warn_component_unused", "0")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| {
            format!(
                "failed to launch SCOTT executable `{}`",
                request.scott_bin.display()
            )
        })?;

    let started = std::time::Instant::now();
    let status = if let Some(timeout_secs) = request.timeout_secs {
        let timeout = Duration::from_secs(timeout_secs);
        loop {
            if let Some(status) = child
                .try_wait()
                .context("failed to poll SCOTT child process")?
            {
                break status;
            }
            if started.elapsed() > timeout {
                let _ = child.kill();
                let _ = child.wait();
                return Err(anyhow!(
                    "SCOTT search timed out after {} seconds in `{}`",
                    timeout_secs,
                    request.workdir.display()
                ));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    } else {
        child
            .wait()
            .context("failed while waiting for SCOTT search process")?
    };

    let recovered_partial_run =
        !status.success() && scott_search_has_recoverable_outputs(&request.workdir);
    if !status.success() && !recovered_partial_run {
        let stderr = read_scott_error_excerpt(&request.workdir)?;
        return Err(anyhow!(
            "SCOTT search process failed in `{}`: {}",
            request.workdir.display(),
            stderr
        ));
    }

    export_scott_search_run(
        &request,
        &patch_summary,
        exit_code_from_status(status),
        signal_from_status(status),
        recovered_partial_run,
    )?;

    let summary = serde_json::json!({
        "job_type": request.job_type.as_str(),
        "job_type_code": request.job_type.job_code(),
        "mode": request.trace_mode_label(),
        "run_dir": request.run_dir,
        "workdir": request.workdir,
        "exit_code": exit_code_from_status(status),
        "signal": signal_from_status(status),
        "recovered_partial_run": recovered_partial_run,
        "terminated_by_signal": status.code().is_none(),
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&summary)
            .context("failed to serialize SCOTT search summary")?
    );
    Ok(())
}

fn exit_code_from_status(status: ExitStatus) -> Option<i32> {
    status.code()
}

fn signal_from_status(status: ExitStatus) -> Option<i32> {
    #[cfg(unix)]
    {
        status.signal()
    }
    #[cfg(not(unix))]
    {
        let _ = status;
        None
    }
}

fn read_scott_error_excerpt(workdir: &Path) -> Result<String> {
    for candidate in [workdir.join("KLMC.err"), workdir.join("KLMC.out")] {
        if !candidate.exists() {
            continue;
        }
        let content = fs::read_to_string(&candidate)
            .with_context(|| format!("failed to read `{}`", candidate.display()))?;
        let excerpt = content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .rev()
            .take(12)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join(" | ");
        if !excerpt.is_empty() {
            return Ok(excerpt.chars().take(500).collect());
        }
    }

    Ok("no SCOTT stderr excerpt available".to_string())
}

fn patch_scott_run_job(
    path: &Path,
    request: &ResolvedScottSearchRun,
    file_cfg: Option<&ScottSearchConfigFile>,
) -> Result<RunJobPatchSummary> {
    let mut text = fs::read_to_string(path)
        .with_context(|| format!("failed to read SCOTT run.job template `{}`", path.display()))?;
    let mut summary = RunJobPatchSummary::default();

    apply_setting(
        &mut text,
        &mut summary,
        "JOB_TYPE",
        Some(request.job_type.job_code().to_string()),
        "rust_mode_bridge",
    );
    apply_setting(
        &mut text,
        &mut summary,
        "POPULATION",
        request
            .shared
            .population
            .map(|value| value.max(1).to_string()),
        source_for_field("population", request, file_cfg),
    );
    apply_setting(
        &mut text,
        &mut summary,
        "SEED",
        request.shared.seed.map(|value| value.to_string()),
        source_for_field("seed", request, file_cfg),
    );
    apply_setting(
        &mut text,
        &mut summary,
        "R_TEMPERATURE",
        request.shared.temperature.map(|value| value.to_string()),
        source_for_field("temperature", request, file_cfg),
    );
    apply_setting(
        &mut text,
        &mut summary,
        "R_BH_STEPSIZE",
        request.shared.step_size.map(|value| value.to_string()),
        source_for_field("step_size", request, file_cfg),
    );
    apply_setting(
        &mut text,
        &mut summary,
        "R_MAX_CLUSTER_BOUNDARY",
        request.shared.boundary.map(|value| value.to_string()),
        source_for_field("boundary", request, file_cfg),
    );
    apply_setting(
        &mut text,
        &mut summary,
        "COLLAPSE",
        request.shared.collapse.map(|value| value.to_string()),
        source_for_field("collapse", request, file_cfg),
    );
    apply_setting(
        &mut text,
        &mut summary,
        "FRAGMENT",
        request.shared.fragment.map(|value| value.to_string()),
        source_for_field("fragment", request, file_cfg),
    );
    apply_setting(
        &mut text,
        &mut summary,
        "DSPECIES",
        request.shared.dspecies.map(|value| value.to_string()),
        source_for_field("dspecies", request, file_cfg),
    );
    apply_setting(
        &mut text,
        &mut summary,
        "OUTPUT_LEVEL",
        request.shared.output_level.map(|value| value.to_string()),
        source_for_field("output_level", request, file_cfg),
    );
    apply_setting(
        &mut text,
        &mut summary,
        "L_USE_TOP_ANALYSIS",
        request.shared.use_top_analysis.map(scott_bool),
        source_for_field("use_top_analysis", request, file_cfg),
    );
    apply_setting(
        &mut text,
        &mut summary,
        "C_HASHKEY_RADIUS",
        request.shared.hashkey_radius.clone(),
        source_for_field("hashkey_radius", request, file_cfg),
    );
    apply_setting(
        &mut text,
        &mut summary,
        "HASHKEY_RADIUS_CONST",
        request
            .shared
            .hashkey_radius_const
            .map(|value| value.to_string()),
        source_for_field("hashkey_radius_const", request, file_cfg),
    );
    apply_setting(
        &mut text,
        &mut summary,
        "EVALUATOR_BACKEND",
        request.backend.evaluator_backend.clone(),
        source_for_field("backend.evaluator_backend", request, file_cfg),
    );
    apply_setting(
        &mut text,
        &mut summary,
        "JANUS_PYTHON",
        request
            .backend
            .janus_python
            .as_ref()
            .map(|value| format!("'{}'", value.display())),
        source_for_field("backend.janus_python", request, file_cfg),
    );
    apply_setting(
        &mut text,
        &mut summary,
        "JANUS_ADAPTER",
        request
            .backend
            .janus_adapter
            .as_ref()
            .map(|value| format!("'{}'", value.display())),
        source_for_field("backend.janus_adapter", request, file_cfg),
    );
    apply_setting(
        &mut text,
        &mut summary,
        "JANUS_MODE",
        request.backend.janus_mode.clone(),
        source_for_field("backend.janus_mode", request, file_cfg),
    );
    apply_setting(
        &mut text,
        &mut summary,
        "JANUS_ARCH",
        request.backend.janus_arch.clone(),
        source_for_field("backend.janus_arch", request, file_cfg),
    );
    apply_setting(
        &mut text,
        &mut summary,
        "JANUS_MODEL",
        request.backend.janus_model.clone(),
        source_for_field("backend.janus_model", request, file_cfg),
    );
    apply_setting(
        &mut text,
        &mut summary,
        "JANUS_DEVICE",
        request.backend.janus_device.clone(),
        source_for_field("backend.janus_device", request, file_cfg),
    );
    apply_setting(
        &mut text,
        &mut summary,
        "JANUS_DTYPE",
        request.backend.janus_dtype.clone(),
        source_for_field("backend.janus_dtype", request, file_cfg),
    );
    apply_setting(
        &mut text,
        &mut summary,
        "JANUS_FMAX",
        request.backend.janus_fmax.map(|value| value.to_string()),
        source_for_field("backend.janus_fmax", request, file_cfg),
    );
    apply_setting(
        &mut text,
        &mut summary,
        "JANUS_STEPS",
        request.backend.janus_steps.map(|value| value.to_string()),
        source_for_field("backend.janus_steps", request, file_cfg),
    );

    match request.trace_mode {
        Some(SearchMode::Bh) => {
            apply_setting(
                &mut text,
                &mut summary,
                "GA_GEN_STATS",
                Some(".FALSE.".to_string()),
                "rust_mode_bridge",
            );
            apply_setting(
                &mut text,
                &mut summary,
                "N_MAX_BH_STEPS",
                request.bh.bh_steps.map(|value| value.max(1).to_string()),
                source_for_field("bh_steps", request, file_cfg),
            );
            apply_setting(
                &mut text,
                &mut summary,
                "N_GENERATIONS",
                Some("1".to_string()),
                "rust_mode_bridge",
            );
            apply_setting(
                &mut text,
                &mut summary,
                "BH_METHOD",
                request.bh.method.clone(),
                source_for_field("bh.method", request, file_cfg),
            );
            apply_setting(
                &mut text,
                &mut summary,
                "BH_ACCEPT",
                request.bh.accept.clone(),
                source_for_field("bh.accept", request, file_cfg),
            );
            apply_setting(
                &mut text,
                &mut summary,
                "N_BH_DYNAMIC_THRESHOLD",
                request.bh.dynamic_threshold.map(|value| value.to_string()),
                source_for_field("bh.dynamic_threshold", request, file_cfg),
            );
            apply_setting(
                &mut text,
                &mut summary,
                "N_BH_MOVECLASS_THRESHOLD",
                request
                    .bh
                    .moveclass_threshold
                    .map(|value| value.to_string()),
                source_for_field("bh.moveclass_threshold", request, file_cfg),
            );
            apply_setting(
                &mut text,
                &mut summary,
                "N_TEMPERATURE",
                request.bh.n_temperature.map(|value| value.to_string()),
                source_for_field("bh.n_temperature", request, file_cfg),
            );
            apply_setting(
                &mut text,
                &mut summary,
                "SA_TEMP_SCALE",
                request.bh.sa_temp_scale.map(|value| value.to_string()),
                source_for_field("bh.sa_temp_scale", request, file_cfg),
            );
            apply_setting(
                &mut text,
                &mut summary,
                "N_HIGH_TEMPERATURE",
                request.bh.n_high_temperature.map(|value| value.to_string()),
                source_for_field("bh.n_high_temperature", request, file_cfg),
            );
            apply_setting(
                &mut text,
                &mut summary,
                "N_LOW_TEMPERATURE",
                request.bh.n_low_temperature.map(|value| value.to_string()),
                source_for_field("bh.n_low_temperature", request, file_cfg),
            );
            apply_setting(
                &mut text,
                &mut summary,
                "MC_STEPS_ONLY",
                request.bh.mc_steps_only.map(scott_bool),
                source_for_field("bh.mc_steps_only", request, file_cfg),
            );
            apply_setting(
                &mut text,
                &mut summary,
                "PROB_SWITCH_ATOMS",
                request.bh.prob_switch_atoms.map(|value| value.to_string()),
                source_for_field("bh.prob_switch_atoms", request, file_cfg),
            );
            apply_setting(
                &mut text,
                &mut summary,
                "PROB_SWITCH_CATIONS",
                request
                    .bh
                    .prob_switch_cations
                    .map(|value| value.to_string()),
                source_for_field("bh.prob_switch_cations", request, file_cfg),
            );
            apply_setting(
                &mut text,
                &mut summary,
                "PROB_MUTATE_CLUSTER",
                request
                    .bh
                    .prob_mutate_cluster
                    .map(|value| value.to_string()),
                source_for_field("bh.prob_mutate_cluster", request, file_cfg),
            );
            apply_setting(
                &mut text,
                &mut summary,
                "PROB_TWIST_CLUSTER",
                request.bh.prob_twist_cluster.map(|value| value.to_string()),
                source_for_field("bh.prob_twist_cluster", request, file_cfg),
            );
            apply_setting(
                &mut text,
                &mut summary,
                "PROB_MUTATE_CELL",
                request.bh.prob_mutate_cell.map(|value| value.to_string()),
                source_for_field("bh.prob_mutate_cell", request, file_cfg),
            );
        }
        Some(SearchMode::Ga) => {
            apply_setting(
                &mut text,
                &mut summary,
                "GA_GEN_STATS",
                Some(
                    request
                        .ga
                        .gen_stats
                        .map(scott_bool)
                        .unwrap_or_else(|| ".TRUE.".to_string()),
                ),
                if request.ga.gen_stats.is_some() {
                    source_for_field("ga.gen_stats", request, file_cfg)
                } else {
                    "rust_mode_bridge"
                },
            );
            apply_setting(
                &mut text,
                &mut summary,
                "GA_SAVE_POP_FREQ",
                Some(
                    request
                        .ga
                        .save_pop_freq
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "1".to_string()),
                ),
                if request.ga.save_pop_freq.is_some() {
                    source_for_field("ga.save_pop_freq", request, file_cfg)
                } else {
                    "rust_mode_bridge"
                },
            );
            apply_setting(
                &mut text,
                &mut summary,
                "GA_SAVE_POP_OUT_FORMAT",
                request
                    .ga
                    .save_pop_out_format
                    .as_ref()
                    .map(|value| format!("'{}'", value)),
                source_for_field("ga.save_pop_out_format", request, file_cfg),
            );
            apply_setting(
                &mut text,
                &mut summary,
                "N_TOURNAMENT_SIZE",
                request
                    .ga
                    .tournament_size
                    .map(|value| value.to_string())
                    .or_else(|| {
                        request
                            .shared
                            .population
                            .map(|value| value.clamp(1, 10).to_string())
                    }),
                if request.ga.tournament_size.is_some() {
                    source_for_field("ga.tournament_size", request, file_cfg)
                } else if request.shared.population.is_some() {
                    "rust_mode_bridge"
                } else {
                    "template"
                },
            );
            apply_setting(
                &mut text,
                &mut summary,
                "N_MAX_BH_STEPS",
                Some("1".to_string()),
                "rust_mode_bridge",
            );
            apply_setting(
                &mut text,
                &mut summary,
                "N_GENERATIONS",
                request
                    .ga
                    .ga_generations
                    .map(|value| value.max(1).to_string()),
                source_for_field("ga_generations", request, file_cfg),
            );
            apply_setting(
                &mut text,
                &mut summary,
                "N_ELITES",
                request.ga.n_elites.map(|value| value.to_string()),
                source_for_field("ga.n_elites", request, file_cfg),
            );
            apply_setting(
                &mut text,
                &mut summary,
                "GLOBAL_MIN",
                request.ga.global_min.map(|value| value.to_string()),
                source_for_field("ga.global_min", request, file_cfg),
            );
            apply_setting(
                &mut text,
                &mut summary,
                "R_REINSERT_ELITES_RATIO",
                request
                    .ga
                    .reinsert_elites_ratio
                    .map(|value| value.to_string()),
                source_for_field("ga.reinsert_elites_ratio", request, file_cfg),
            );
            apply_setting(
                &mut text,
                &mut summary,
                "R_POP_REPLACEMENT_RATIO",
                request
                    .ga
                    .pop_replacement_ratio
                    .map(|value| value.to_string()),
                source_for_field("ga.pop_replacement_ratio", request, file_cfg),
            );
            apply_setting(
                &mut text,
                &mut summary,
                "GA_MUTATION_RATIO",
                request.ga.mutation_ratio.map(|value| value.to_string()),
                source_for_field("ga.mutation_ratio", request, file_cfg),
            );
            apply_setting(
                &mut text,
                &mut summary,
                "GA_MUT_SELFCROSS_RATIO",
                request
                    .ga
                    .mutate_selfcross_ratio
                    .map(|value| value.to_string()),
                source_for_field("ga.mutate_selfcross_ratio", request, file_cfg),
            );
            apply_setting(
                &mut text,
                &mut summary,
                "GA_STEPSIZE",
                request.ga.ga_step_size.map(|value| value.to_string()),
                source_for_field("ga.ga_step_size", request, file_cfg),
            );
            apply_setting(
                &mut text,
                &mut summary,
                "GA_CROSS_CHECK",
                request.ga.cross_check.map(scott_bool),
                source_for_field("ga.cross_check", request, file_cfg),
            );
            apply_setting(
                &mut text,
                &mut summary,
                "GA_CROSS_ATTEMPTS",
                request.ga.cross_attempts.map(|value| value.to_string()),
                source_for_field("ga.cross_attempts", request, file_cfg),
            );
            apply_setting(
                &mut text,
                &mut summary,
                "GA_CROSS_1_2D_RATIO",
                request.ga.cross_1_2d_ratio.map(|value| value.to_string()),
                source_for_field("ga.cross_1_2d_ratio", request, file_cfg),
            );
            apply_setting(
                &mut text,
                &mut summary,
                "GA_ENFORCE_MIN_SIZE",
                request.ga.enforce_min_size.map(|value| value.to_string()),
                source_for_field("ga.enforce_min_size", request, file_cfg),
            );
            apply_setting(
                &mut text,
                &mut summary,
                "GA_INI_POP_ATTEMPTS",
                request.ga.ini_pop_attempts.map(|value| value.to_string()),
                source_for_field("ga.ini_pop_attempts", request, file_cfg),
            );
            apply_setting(
                &mut text,
                &mut summary,
                "R_ENERGY_TOLERANCE",
                request.ga.energy_tolerance.map(|value| value.to_string()),
                source_for_field("ga.energy_tolerance", request, file_cfg),
            );
            apply_setting(
                &mut text,
                &mut summary,
                "L_CMPR_PMOI",
                request.ga.cmpr_pmoi.map(scott_bool),
                source_for_field("ga.cmpr_pmoi", request, file_cfg),
            );
            apply_setting(
                &mut text,
                &mut summary,
                "R_PMOI_TOLERANCE",
                request.ga.pmoi_tolerance.map(|value| value.to_string()),
                source_for_field("ga.pmoi_tolerance", request, file_cfg),
            );
        }
        None => {}
    }

    fs::write(path, text)
        .with_context(|| format!("failed to write patched SCOTT run.job `{}`", path.display()))?;
    Ok(summary)
}

fn source_for_field(
    field: &str,
    request: &ResolvedScottSearchRun,
    file_cfg: Option<&ScottSearchConfigFile>,
) -> &'static str {
    if request.cli_overrides.iter().any(|entry| entry == field) {
        "cli"
    } else if file_cfg.is_some() {
        "config"
    } else {
        "template"
    }
}

fn scott_bool(value: bool) -> String {
    if value {
        ".TRUE.".to_string()
    } else {
        ".FALSE.".to_string()
    }
}

fn apply_setting(
    text: &mut String,
    summary: &mut RunJobPatchSummary,
    key: &str,
    value: Option<String>,
    source: &str,
) {
    let Some(value) = value else {
        return;
    };
    *text = replace_scott_setting(std::mem::take(text), key, &value);
    summary.overrides.insert(
        key.to_string(),
        RunJobOverrideRecord {
            value,
            source: source.to_string(),
        },
    );
}

fn replace_scott_setting(mut text: String, key: &str, value: &str) -> String {
    let replacement = format!("{key}:{value}");
    let mut replaced = false;
    let updated = text
        .lines()
        .map(|line| {
            if line.trim_start().starts_with(&format!("{key}:")) && !replaced {
                replaced = true;
                replacement.clone()
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    text = updated;
    if !replaced {
        text.push('\n');
        text.push_str(&replacement);
        text.push('\n');
    }
    text
}

fn export_scott_search_run(
    request: &ResolvedScottSearchRun,
    patch_summary: &RunJobPatchSummary,
    exit_code: Option<i32>,
    signal: Option<i32>,
    recovered_partial_run: bool,
) -> Result<()> {
    let export_paths = application::scott_run_export::prepare_scott_export_paths(&request.run_dir)?;
    application::scott_run_export::stage_scott_search_raw_outputs(request, &export_paths)?;
    if let Some(report) = application::scott_topology_export::build_topology_nearness_report(
        request,
        &export_paths.raw_dir,
        &export_paths.structures_dir,
    )? {
        fs::write(
            export_paths.raw_dir.join("topology_nearness_report.json"),
            serde_json::to_string_pretty(&report)
                .context("failed to serialize topology nearness report")?,
        )
        .with_context(|| {
            format!(
                "failed to write topology_nearness_report.json in `{}`",
                export_paths.raw_dir.display()
            )
        })?;
    }
    application::scott_run_export::write_scott_search_manifest(
        request,
        patch_summary,
        application::scott_run_export::ScottProcessSummary {
            exit_code,
            signal,
            recovered_partial_run,
        },
    )?;

    export_scott_traces(request)?;
    Ok(())
}

fn build_ga_artifact_hashkey_config(
    use_dreadnaut_keys: bool,
    radius_mode: &str,
    radius_const: f64,
    scratch_dir: PathBuf,
) -> Result<Option<application::rust_janus_ga_checkpoint::GaHashkeyArtifactConfig>> {
    if !use_dreadnaut_keys {
        return Ok(None);
    }
    let bundled_path = patina_dreadnaut::bundled_dreadnaut_path();
    let dreadnaut_path = application::driver_support::verify_hashkey_dreadnaut_adapter(
        &bundled_path,
        "GA hashkey artifact preflight",
    )?;
    Ok(Some(
        application::rust_janus_ga_checkpoint::GaHashkeyArtifactConfig {
            radius_mode: radius_mode.to_string(),
            radius_const,
            dreadnaut_path,
            scratch_dir,
        },
    ))
}

#[cfg(test)]
fn build_dreadnaut_graph_text(
    candidate: &Candidate,
    radius: f64,
    atom_specs: &[AtomSpecRecord],
) -> String {
    let atom_specs = atom_specs
        .iter()
        .map(|record| patina_search::AtomSpec {
            species: record.species.clone(),
            covalent_radius: record.covalent_radius,
            ionic_radius: record.ionic_radius,
        })
        .collect::<Vec<_>>();
    patina_dreadnaut::build_dreadnaut_graph_text(candidate, radius, &atom_specs)
}

fn candidate_from_xyz(path: &Path) -> Result<Candidate> {
    application::scott_topology_export::parse_candidate_snapshot(path)
}

fn lowercase_extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
}

fn path_has_supported_extension(path: &Path, supported_extensions: &[&str]) -> bool {
    lowercase_extension(path)
        .as_deref()
        .is_some_and(|extension| supported_extensions.contains(&extension))
}

fn is_supported_candidate_snapshot_path(path: &Path) -> bool {
    path_has_supported_extension(path, SUPPORTED_CANDIDATE_SNAPSHOT_EXTENSIONS)
}

fn supported_candidate_snapshot_extensions_label() -> String {
    SUPPORTED_CANDIDATE_SNAPSHOT_EXTENSIONS
        .iter()
        .map(|extension| format!(".{extension}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn load_candidate_from_snapshot_path(path: &Path) -> Result<Candidate> {
    match lowercase_extension(path).as_deref() {
        Some("json") => read_candidate_json(path),
        Some("xyz" | "extxyz" | "cif" | "car" | "arc" | "can") => candidate_from_xyz(path),
        _ => Err(anyhow!(
            "unsupported candidate snapshot `{}`; expected one of {}",
            path.display(),
            supported_candidate_snapshot_extensions_label()
        )),
    }
}

fn collect_pending_candidate_paths(
    explicit_paths: &[PathBuf],
    candidate_dir: Option<&Path>,
) -> Result<Vec<PathBuf>> {
    let mut paths = explicit_paths.to_vec();
    if let Some(candidate_dir) = candidate_dir {
        let candidate_dir = absolutize_path(candidate_dir)?;
        let mut dir_paths = fs::read_dir(&candidate_dir)
            .with_context(|| {
                format!(
                    "failed to read pending candidate directory `{}`",
                    candidate_dir.display()
                )
            })?
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| path.is_file() && is_supported_candidate_snapshot_path(path))
            .collect::<Vec<_>>();
        dir_paths.sort();
        paths.extend(dir_paths);
    }
    if paths.is_empty() {
        bail!("at least one `--pending-candidate` or a non-empty `--pending-candidate-dir` is required");
    }
    Ok(paths)
}

fn build_emulate_feature_projector(
    projector: EmulateFeatureProjectorCli,
    uv_bin: Option<&Path>,
    project_dir: Option<&Path>,
    environment_name: &str,
) -> Result<Box<dyn patina_emulate::FeatureProjectionPort>> {
    let python_config = patina_emulate::PythonFeatureProjectorConfig {
        uv_bin: uv_bin
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("uv")),
        project_dir: absolutize_path(
            project_dir.unwrap_or_else(|| Path::new("crates/patina-emulate/python")),
        )?,
        environment_name: environment_name.to_string(),
        extras: vec!["autoemulate".into()],
        native_tls: true,
        timeout: Some(Duration::from_secs(300)),
    };
    Ok(patina_emulate::build_feature_projector_with_python_config(
        projector.into(),
        python_config,
    ))
}

fn load_production_seed_inputs(
    candidate_json: &[PathBuf],
    restart_dir: Option<&Path>,
    production_cfg: &application::scott_production::ProductionRunConfig,
    master_template: &Path,
    atom_specs: Option<&[AtomSpecRecord]>,
) -> Result<Vec<application::scott_production::ProductionSeedInput>> {
    let data_mining_plan = if production_cfg.data_mining_enabled {
        let atom_specs = atom_specs.ok_or_else(|| {
            anyhow!(
                "`DM_FLAG` is enabled for staged Scott production but no `atoms.in` template was provided"
            )
        })?;
        application::scott_production::build_data_mining_plan(production_cfg, atom_specs)?
    } else {
        None
    };

    let master_species = if production_cfg.enforce_master {
        let master_template = Utf8PathBuf::from_path_buf(master_template.to_path_buf())
            .map_err(|path| anyhow!("path `{}` is not valid UTF-8", path.display()))?;
        let layout = MasterTemplateLayout::from_path(&master_template)
            .with_context(|| format!("failed to parse master template `{}`", master_template))?;
        let mut species = application::scott_production::extract_master_species(&layout);
        if let Some(plan) = data_mining_plan.as_ref() {
            application::scott_production::apply_data_mining_to_species(&mut species, plan);
        }
        Some(species)
    } else {
        None
    };

    let mut seeds = Vec::new();
    for path in candidate_json {
        let mut candidate = read_candidate_json(path)?;
        if let Some(master_species) = master_species.as_ref() {
            application::scott_production::enforce_master_species(&mut candidate, master_species)?;
        }
        if let Some(plan) = data_mining_plan.as_ref() {
            application::scott_production::apply_data_mining(&mut candidate, plan)?;
        }
        let source_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(ToOwned::to_owned);
        seeds.push(
            application::scott_production::ProductionSeedInput::inline_candidate(
                source_name,
                candidate,
            ),
        );
    }

    if let Some(restart_dir) = restart_dir {
        let restart_dir = absolutize_path(restart_dir)?;
        let done = application::scott_production::read_restart_done_entries(&restart_dir)?;
        for path in application::scott_production::collect_restart_seed_paths(&restart_dir)? {
            let source_name = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| {
                    anyhow!(
                        "restart seed path `{}` has no valid file name",
                        path.display()
                    )
                })?
                .to_string();
            if done.contains(&source_name) {
                continue;
            }
            let mut candidate =
                application::scott_topology_export::parse_candidate_snapshot(&path)?;
            if let Some(master_species) = master_species.as_ref() {
                application::scott_production::enforce_master_species(
                    &mut candidate,
                    master_species,
                )?;
            }
            if let Some(plan) = data_mining_plan.as_ref() {
                application::scott_production::apply_data_mining(&mut candidate, plan)?;
            }
            seeds.push(
                application::scott_production::ProductionSeedInput::restart_artifact(
                    source_name,
                    candidate,
                ),
            );
        }
    }

    if seeds.is_empty() {
        return Err(anyhow!(
            "no staged production seeds were found; provide `--candidate-json` and/or `--restart-dir` with supported structure files"
        ));
    }

    Ok(seeds)
}

fn export_scott_traces(request: &ResolvedScottSearchRun) -> Result<()> {
    if request.trace_mode == Some(SearchMode::Bh) {
        let target = request.run_dir.join("traces").join("walker_trace.csv");
        let mut file = fs::File::create(&target)
            .with_context(|| format!("failed to create `{}`", target.display()))?;
        writeln!(
            file,
            "step,walker_id,accepted,energy,best_energy,temperature,step_size,move_class,label,reason,energy_source"
        )?;
        let bh_trace = application::scott_bh_artifacts::recover_bh_trace(&request.workdir)?;
        application::scott_bh_artifacts::write_bh_walker_state_artifact(
            &request.workdir,
            &request.run_dir.join("raw"),
            &bh_trace.decisions,
        )?;

        let mut best = f64::INFINITY;
        let mut wrote_any = false;

        if let Some(energy) = bh_trace.recovered_a1 {
            best = best.min(energy);
            writeln!(
                file,
                "0,0,true,{:.10},{:.10},{},{},initial_state,A1,,gout_recovery",
                energy,
                best,
                request.shared.temperature.unwrap_or(0.0),
                request.shared.step_size.unwrap_or(0.0),
            )?;
            wrote_any = true;
        }

        for decision in bh_trace.decisions {
            let label = format!("A{}", decision.step);
            let reason = bh_trace
                .reasons
                .iter()
                .find(|entry| entry.step == decision.step)
                .map(|entry| entry.reason.clone())
                .unwrap_or_else(String::new);

            let (energy, energy_source) = if decision.step == 2 {
                match bh_trace.recovered_a2 {
                    Some(value) => (Some(value), "gout_recovery"),
                    None => (bh_trace.energy_rows.get(&label).copied(), "energy_file"),
                }
            } else {
                match bh_trace
                    .energy_rows
                    .get(&label)
                    .copied()
                    .filter(|value| value.abs() > 1.0e-12)
                {
                    Some(value) => (Some(value), "energy_file"),
                    None => (None, "undefined"),
                }
            };

            if let Some(energy) = energy {
                if decision.accepted {
                    best = best.min(energy);
                } else if best.is_infinite() {
                    best = energy;
                }
                writeln!(
                    file,
                    "{},{},{},{:.10},{:.10},{},{},scott_search,{},{},{}",
                    decision.step,
                    0,
                    decision.accepted,
                    energy,
                    best,
                    request.shared.temperature.unwrap_or(0.0),
                    request.shared.step_size.unwrap_or(0.0),
                    label,
                    application::scott_legacy_audit::sanitize_csv_field(&reason),
                    energy_source,
                )?;
            } else {
                let best_field = if best.is_finite() {
                    format!("{best:.10}")
                } else {
                    String::new()
                };
                writeln!(
                    file,
                    "{},{},{},,{},{},{},scott_search,{},{},{}",
                    decision.step,
                    0,
                    decision.accepted,
                    best_field,
                    request.shared.temperature.unwrap_or(0.0),
                    request.shared.step_size.unwrap_or(0.0),
                    label,
                    application::scott_legacy_audit::sanitize_csv_field(&reason),
                    energy_source,
                )?;
            }
            wrote_any = true;
        }

        if wrote_any {
            return Ok(());
        }

        let energies_fallback = request.workdir.join("top_structures").join("energies");
        let statistics_fallback = request.workdir.join("top_structures").join("statistics");
        let rows = if statistics_fallback.exists() {
            application::scott_legacy_audit::parse_top_structure_statistics(&statistics_fallback)?
        } else if energies_fallback.exists() {
            application::scott_legacy_audit::parse_top_structure_energies(&energies_fallback)?
        } else {
            Vec::new()
        };
        if !rows.is_empty() {
            let mut best = f64::INFINITY;
            for (idx, (label, energy)) in rows.into_iter().enumerate() {
                best = best.min(energy);
                writeln!(
                    file,
                    "{},{},true,{:.10},{:.10},{},{},top_structures,{},,top_structures",
                    idx,
                    0,
                    energy,
                    best,
                    request.shared.temperature.unwrap_or(0.0),
                    request.shared.step_size.unwrap_or(0.0),
                    label
                )?;
            }
            return Ok(());
        }

        if let Some((label, energy)) =
            application::scott_legacy_audit::recover_scott_energy_snapshot(&request.workdir)?
        {
            writeln!(
                file,
                "0,0,true,{:.10},{:.10},{},{},gout_recovery,{},,snapshot_recovery",
                energy,
                energy,
                request.shared.temperature.unwrap_or(0.0),
                request.shared.step_size.unwrap_or(0.0),
                label
            )?;
        }
    } else if request.trace_mode == Some(SearchMode::Ga) {
        let ga_stats_files = application::scott_ga_artifacts::collect_ga_statistics_files(
            &request.workdir.join("run"),
        )?;
        if !ga_stats_files.is_empty() {
            let target = request
                .run_dir
                .join("traces")
                .join("generation_metrics.csv");
            let mut file = fs::File::create(&target)
                .with_context(|| format!("failed to create `{}`", target.display()))?;
            writeln!(
                file,
                "generation,best_energy,mean_energy,worst_energy,n_converged,n_failed,population_size,best_cluster_id,best_origin,unique_relaxed_hashkeys,unique_origins"
            )?;
            let mut rows = Vec::new();
            for ga_stats_path in ga_stats_files {
                if let Some(row) =
                    application::scott_ga_artifacts::summarize_ga_statistics(&ga_stats_path)?
                {
                    rows.push(row);
                }
            }
            rows.sort_by_key(|row| row.generation);
            for row in rows {
                writeln!(
                    file,
                    "{},{:.10},{:.10},{:.10},{},{},{},{},{},{},{}",
                    row.generation,
                    row.best_energy,
                    row.mean_energy,
                    row.worst_energy,
                    row.n_converged,
                    row.n_failed,
                    row.population_size,
                    application::scott_legacy_audit::sanitize_csv_field(&row.best_cluster_id),
                    application::scott_legacy_audit::sanitize_csv_field(&row.best_origin),
                    row.unique_relaxed_hashkeys,
                    row.unique_origins
                )?;
            }
            application::scott_legacy_audit::export_janus_generation_traces(request)?;
            application::scott_legacy_audit::export_hashkey_generation_traces(request)?;
            return Ok(());
        }

        let statistics_fallback = request.workdir.join("top_structures").join("statistics");
        let rows = if statistics_fallback.exists() {
            application::scott_legacy_audit::parse_top_structure_statistics(&statistics_fallback)?
        } else {
            Vec::new()
        };
        if !rows.is_empty() {
            let target = request
                .run_dir
                .join("traces")
                .join("generation_metrics.csv");
            let mut file = fs::File::create(&target)
                .with_context(|| format!("failed to create `{}`", target.display()))?;
            writeln!(
                file,
                "generation,best_energy,mean_energy,worst_energy,n_converged,n_failed,population_size"
            )?;
            let energies = rows.iter().map(|(_, energy)| *energy).collect::<Vec<_>>();
            let best = energies.iter().copied().fold(f64::INFINITY, f64::min);
            let worst = energies.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let mean = energies.iter().sum::<f64>() / energies.len() as f64;
            writeln!(
                file,
                "0,{:.10},{:.10},{:.10},{},0,{}",
                best,
                mean,
                worst,
                energies.len(),
                request
                    .shared
                    .population
                    .unwrap_or(energies.len())
                    .max(energies.len())
            )?;
        } else if let Some((_, energy)) =
            application::scott_legacy_audit::recover_scott_energy_snapshot(&request.workdir)?
        {
            let target = request
                .run_dir
                .join("traces")
                .join("generation_metrics.csv");
            let mut file = fs::File::create(&target)
                .with_context(|| format!("failed to create `{}`", target.display()))?;
            writeln!(
                file,
                "generation,best_energy,mean_energy,worst_energy,n_converged,n_failed,population_size"
            )?;
            writeln!(
                file,
                "0,{:.10},{:.10},{:.10},1,0,{}",
                energy,
                energy,
                energy,
                request.shared.population.unwrap_or(1).max(1)
            )?;
        }
    }
    Ok(())
}

fn scott_search_has_recoverable_outputs(workdir: &Path) -> bool {
    let structured_outputs = [
        workdir.join("run").join("gulp_klmc.gout"),
        workdir.join("run").join("0").join("gulp_klmc.gout"),
        workdir.join("run").join("energy"),
        workdir.join("run").join("0").join("energy"),
        workdir.join("run").join("logBest"),
        workdir.join("run").join("0").join("logBest"),
        workdir.join("top_structures").join("energies"),
    ];
    if structured_outputs.iter().any(|path| path.exists()) {
        return true;
    }

    workdir.join("KLMC.out").exists()
        && (workdir.join("run").is_dir()
            || workdir.join("top_structures").is_dir()
            || workdir.join("KLMC.log").exists())
}

fn reset_scott_search_workdir(workdir: &Path) -> Result<()> {
    if !workdir.exists() {
        return Ok(());
    }

    for name in [
        "data",
        "run",
        "top_structures",
        "restart",
        "GA-restart",
        "KLMC.err",
        "KLMC.log",
        "KLMC.out",
        "number-of-line-searches",
    ] {
        let path = workdir.join(name);
        if !path.exists() {
            continue;
        }
        if path.is_dir() {
            fs::remove_dir_all(&path)
                .with_context(|| format!("failed to reset `{}`", path.display()))?;
        } else {
            fs::remove_file(&path)
                .with_context(|| format!("failed to reset `{}`", path.display()))?;
        }
    }
    Ok(())
}

fn evaluate_scott(args: EvaluateScottArgs) -> Result<()> {
    let candidate = read_candidate_json(&args.candidate_json)?;

    fs::create_dir_all(&args.workdir).with_context(|| {
        format!(
            "failed to create SCOTT sandbox workdir `{}`",
            args.workdir.display()
        )
    })?;

    let scott_bin = absolutize_path(&args.scott_bin)?;
    let master_gin_template = absolutize_path(&args.master_gin_template)?;
    let run_job_template = absolutize_path(&args.run_job_template)?;
    let atoms_in_template = absolutize_path(&args.atoms_in_template)?;
    let jobs_template = args
        .jobs_template
        .as_ref()
        .map(|path| absolutize_path(path))
        .transpose()?;

    let backend = ScottBackend::new(
        &master_gin_template,
        &scott_bin,
        ScottSandboxTemplate::new(
            &run_job_template,
            &atoms_in_template,
            jobs_template.as_ref(),
        ),
        args.timeout_secs.map(Duration::from_secs),
    )
    .with_context(|| {
        format!(
            "failed to construct ScottBackend for executable `{}`",
            scott_bin.display()
        )
    })?;

    let result = backend
        .evaluate(&candidate, &args.workdir)
        .with_context(|| {
            format!(
                "SCOTT evaluation failed for candidate `{}` in `{}`",
                candidate.label,
                args.workdir.display()
            )
        })?;

    println!(
        "{}",
        serde_json::to_string_pretty(&result).context("failed to serialize EvalResult to JSON")?
    );

    if let Some(run_dir) = args.run_dir.as_ref() {
        application::single_eval_artifacts::write_single_eval_artifacts(
            run_dir,
            &application::single_eval_artifacts::SingleEvalArtifactArgs {
                workdir: args.workdir.clone(),
                mode: args.mode.clone(),
                system: args.system.clone(),
                engine: EvalBackendKind::Scott.engine_label().to_string(),
                backend: EvalBackendKind::Scott.into(),
                backend_metadata: None,
                procedure_state_digest: None,
                failure_reason: None,
            },
            &candidate,
            Some(&result),
        )?;
    }

    Ok(())
}

fn evaluate_backend(args: EvaluateBackendArgs) -> Result<()> {
    let candidate_raw = fs::read_to_string(&args.candidate_json).with_context(|| {
        format!(
            "failed to read candidate JSON from `{}`",
            args.candidate_json.display()
        )
    })?;
    let candidate: Candidate = serde_json::from_str(&candidate_raw).with_context(|| {
        format!(
            "failed to parse candidate JSON from `{}`",
            args.candidate_json.display()
        )
    })?;
    candidate.validate().map_err(|err| {
        anyhow!(
            "candidate from `{}` failed validation: {err:?}",
            args.candidate_json.display()
        )
    })?;

    fs::create_dir_all(&args.workdir).with_context(|| {
        format!(
            "failed to create evaluation workdir `{}`",
            args.workdir.display()
        )
    })?;

    let timeout = args.timeout_secs.map(Duration::from_secs);
    let (backend, backend_metadata) = application::backend_runs::build_backend(
        args.backend,
        timeout,
        args.executable.clone(),
        args.master_gin_template.clone(),
        args.run_job_template.clone(),
        args.atoms_in_template.clone(),
        args.jobs_template.clone(),
        args.python_bin.clone(),
        args.janus_adapter_script.clone(),
        args.janus_arch.clone(),
        args.janus_model.clone(),
        args.janus_device.clone(),
        args.janus_dtype.clone(),
        args.janus_mode,
        args.janus_optimizer,
        args.janus_fmax,
        args.janus_steps,
    )?;

    let result = backend
        .evaluate(&candidate, &args.workdir)
        .with_context(|| {
            format!(
                "backend evaluation failed for candidate `{}` in `{}`",
                candidate.label,
                args.workdir.display()
            )
        })?;

    println!(
        "{}",
        serde_json::to_string_pretty(&result).context("failed to serialize EvalResult to JSON")?
    );

    if let Some(run_dir) = args.run_dir.as_ref() {
        application::single_eval_artifacts::write_single_eval_artifacts(
            run_dir,
            &application::single_eval_artifacts::SingleEvalArtifactArgs {
                workdir: args.workdir.clone(),
                mode: args.mode,
                system: args.system,
                engine: args.backend.engine_label().to_string(),
                backend: args.backend.into(),
                backend_metadata,
                procedure_state_digest: None,
                failure_reason: None,
            },
            &candidate,
            Some(&result),
        )?;
    }

    Ok(())
}

fn staged_ga_args_as_rust_janus_args(args: &RunScottStagedGaArgs) -> RunRustJanusSearchArgs {
    RunRustJanusSearchArgs {
        config: args.config.clone(),
        stage_dir: args.stage_dir.clone(),
        base_candidate_json: args.base_candidate_json.clone(),
        base_candidate_path: args.base_candidate_path.clone(),
        workdir: args.workdir.clone(),
        resume_from_checkpoint: args.resume_from_checkpoint.clone(),
        run_dir: args.run_dir.clone(),
        system: args.system.clone(),
        ga_generations: args.ga_generations,
        population: args.population,
        workers: None,
        keep_dirs: args.keep_dirs,
        timeout_secs: args.timeout_secs,
        seed: args.seed,
        temperature: args.temperature,
        step_size: args.step_size,
        duplicate_policy_mode: RustJanusDuplicatePolicyCliMode::ExternalNativeHashkey,
        python_bin: args.python_bin.clone(),
        janus_adapter_script: args.janus_adapter_script.clone(),
        janus_arch: args.janus_arch.clone(),
        janus_model: args.janus_model.clone(),
        janus_device: args.janus_device.clone(),
        janus_dtype: args.janus_dtype.clone(),
        janus_mode: args.janus_mode,
        janus_fmax: args.janus_fmax,
        janus_steps: args.janus_steps,
        janus_optimizer: args.janus_optimizer,
        atoms_in_template: args.atoms_in_template.clone(),
        use_dreadnaut_keys: args.use_dreadnaut_keys,
        hashkey_radius: args.hashkey_radius.clone(),
        hashkey_radius_const: args.hashkey_radius_const,
        pmoi_tolerance: args.pmoi_tolerance,
        enable_pmoi: args.enable_pmoi,
        pop_replacement_ratio: args.pop_replacement_ratio,
        reinsert_elites_ratio: args.reinsert_elites_ratio,
        mutation_ratio: args.mutation_ratio,
        mut_selfcross_ratio: args.mut_selfcross_ratio,
        max_repop_attempts: args.max_repop_attempts,
        crossover_attempts: args.crossover_attempts,
        tournament_size_min: args.tournament_size_min,
        tournament_size_max: args.tournament_size_max,
        operator_policy_backend: args.runtime_default_backend.clone(),
    }
}

fn hybrid_args_as_staged_ga_args(
    args: &RunHybridGaProductionArgs,
    workdir: PathBuf,
    run_dir: PathBuf,
) -> RunScottStagedGaArgs {
    let inferred_atoms_in_template = args.atoms_in_template.clone().or_else(|| {
        args.scott_input_dir
            .as_ref()
            .map(|dir| dir.join("atoms.in"))
            .filter(|path| path.exists())
    });
    RunScottStagedGaArgs {
        config: None,
        stage_dir: None,
        base_candidate_json: args.base_candidate_json.clone(),
        base_candidate_path: None,
        workdir,
        scott_input_dir: args.scott_input_dir.clone(),
        resume_from_checkpoint: None,
        run_dir,
        system: args.system.clone(),
        ga_generations: args.ga_generations,
        population: args.population,
        keep_dirs: false,
        timeout_secs: args.timeout_secs,
        seed: args.seed,
        temperature: args.temperature,
        step_size: args.step_size,
        python_bin: args.python_bin.clone(),
        janus_adapter_script: args.janus_adapter_script.clone(),
        janus_arch: args.janus_arch.clone(),
        janus_model: args.janus_model.clone(),
        janus_device: args.janus_device.clone(),
        janus_dtype: args.janus_dtype.clone(),
        janus_mode: args.janus_mode,
        janus_fmax: args.janus_fmax,
        janus_steps: args.janus_steps,
        janus_optimizer: args.janus_optimizer,
        executable: args.executable.clone(),
        master_gin_template: args.master_gin_template.clone(),
        run_job_template: args.run_job_template.clone(),
        runtime_default_backend: Some(args.runtime_default_backend.clone()),
        runtime_stage_backend: args.runtime_stage_backend.clone(),
        atoms_in_template: inferred_atoms_in_template,
        use_dreadnaut_keys: args.use_dreadnaut_keys,
        hashkey_radius: args.hashkey_radius.clone(),
        hashkey_radius_const: args.hashkey_radius_const,
        pmoi_tolerance: args.pmoi_tolerance,
        enable_pmoi: args.enable_pmoi,
        pop_replacement_ratio: None,
        reinsert_elites_ratio: None,
        mutation_ratio: None,
        mut_selfcross_ratio: None,
        max_repop_attempts: None,
        crossover_attempts: None,
        tournament_size_min: None,
        tournament_size_max: None,
        emulate_uncertainty_gate: false,
        emulate_campaign_id: None,
        emulate_branch_id: "branch-ga-1".into(),
        emulate_objective: EmulateLearningObjectiveCli::ReduceUncertainty,
        emulate_fidelity: EmulateFidelityCli::GulpReference,
        emulate_feature_projector: EmulateFeatureProjectorCli::PairDistanceSignature,
        emulate_target_name: "energy".into(),
        emulate_target_unit: "eV".into(),
        emulate_model_variant: SurrogateModelVariantCli::Whitened,
        emulate_surrogate_objective: SurrogateObjectiveCli::Minimize,
        emulate_epochs: 120,
        emulate_num_inducing: 24,
        emulate_batch_size: 16,
        emulate_lr: 0.05,
        emulate_n_bootstraps: 1,
        emulate_random_seed: 42,
        emulate_log_level: "warning".into(),
        emulate_uv_bin: None,
        emulate_project_dir: None,
        emulate_environment: "autoemulate".into(),
        emulate_timeout_secs: 300,
        emulate_warmup_generations: 1,
        emulate_min_training_observations: 24,
        emulate_uncertainty_threshold: 0.5,
    }
}

fn hybrid_args_as_production_args(
    args: &RunHybridGaProductionArgs,
    workdir: PathBuf,
    run_dir: PathBuf,
) -> RunScottProductionArgs {
    RunScottProductionArgs {
        candidate_json: Vec::new(),
        restart_dir: None,
        workdir,
        scott_input_dir: args.scott_input_dir.clone(),
        run_dir: Some(run_dir),
        system: args.system.clone(),
        timeout_secs: args.timeout_secs,
        executable: args.executable.clone(),
        master_gin_template: args.master_gin_template.clone(),
        run_job_template: args.run_job_template.clone(),
        atoms_in_template: args.atoms_in_template.clone(),
        runtime_default_backend: args.runtime_default_backend.clone(),
        runtime_stage_backend: args.runtime_stage_backend.clone(),
        python_bin: args.python_bin.clone(),
        janus_adapter_script: args.janus_adapter_script.clone(),
        janus_arch: args.janus_arch.clone(),
        janus_model: args.janus_model.clone(),
        janus_device: args.janus_device.clone(),
        janus_dtype: args.janus_dtype.clone(),
        janus_mode: args.janus_mode,
        janus_fmax: args.janus_fmax,
        janus_steps: args.janus_steps,
        janus_optimizer: args.janus_optimizer,
    }
}

fn rust_janus_ga_core_request_from_args(
    args: &RunRustJanusSearchArgs,
) -> Result<application::rust_janus_ga::RustJanusGaCoreRequest> {
    let base_candidate_input = resolve_cluster_ga_base_candidate(args)?;
    Ok(application::rust_janus_ga::RustJanusGaCoreRequest {
        base_candidate: base_candidate_input
            .as_ref()
            .map(|resolved| resolved.candidate.clone()),
        base_candidate_source_path: base_candidate_input
            .as_ref()
            .map(|resolved| resolved.source_path.clone()),
        workdir: args.workdir.clone(),
        run_dir: args.run_dir.clone(),
        resume_from_checkpoint: args.resume_from_checkpoint.clone(),
        requested_generations: args.ga_generations,
        population_size: args.population,
        seed: args.seed,
        temperature: args.temperature,
        step_size: args.step_size,
        operator_policy_backend: args.operator_policy_backend.clone(),
        janus_mode: args.janus_mode,
        atoms_in_template: args.atoms_in_template.clone(),
        use_dreadnaut_keys: args.use_dreadnaut_keys,
        hashkey_radius: args.hashkey_radius.clone(),
        hashkey_radius_const: args.hashkey_radius_const,
        pmoi_tolerance: args.pmoi_tolerance,
        enable_pmoi: args.enable_pmoi,
        operator_overrides: application::rust_janus_ga::RustJanusGaOperatorOverrides {
            pop_replacement_ratio: args.pop_replacement_ratio,
            reinsert_elites_ratio: args.reinsert_elites_ratio,
            max_repop_attempts: args.max_repop_attempts,
            mutation_ratio: args.mutation_ratio,
            mut_selfcross_ratio: args.mut_selfcross_ratio,
            crossover_attempts: args.crossover_attempts,
            tournament_size_min: args.tournament_size_min,
            tournament_size_max: args.tournament_size_max,
        },
    })
}

fn resolve_cluster_ga_base_candidate(
    args: &RunRustJanusSearchArgs,
) -> Result<Option<application::workflow_input::ResolvedCandidateInput>> {
    if args.resume_from_checkpoint.is_some() {
        return Ok(None);
    }

    let mut config =
        application::workflow_input_config::load_workflow_configuration(args.config.as_deref())?;
    if let Some(stage_dir) = &args.stage_dir {
        config.stage.dir = Some(stage_dir.clone());
    }

    let mut selection = application::workflow_input::CandidateInputSelection::default();
    if let Some(path) = config.ga.base_candidate_json {
        selection.candidate_json = Some(path);
    }
    if let Some(path) = config.ga.cluster_seed_path {
        selection.structure_path = Some(path);
    }
    if selection.is_empty() {
        selection = application::workflow_input::CandidateInputSelection::from_structure_config(
            &config.input,
        );
    }
    if let Some(path) = &args.base_candidate_json {
        selection.candidate_json = Some(path.clone());
        selection.structure_path = None;
    }
    if let Some(path) = &args.base_candidate_path {
        selection.candidate_json = None;
        selection.structure_path = Some(path.clone());
    }

    let resolved = application::workflow_input::resolve_single_candidate_input(
        application::workflow_input::SingleCandidateInputContract::cluster_ga_seed(),
        config.stage.dir.as_deref(),
        &selection,
    )?;
    application::driver_support::ensure_native_scott_search_candidate(
        &resolved.candidate,
        "Rust cluster GA input",
    )?;
    Ok(Some(resolved))
}

fn rust_janus_ga_backend_setup_request_from_args(
    args: &RunRustJanusSearchArgs,
) -> application::rust_janus_ga::RustJanusGaBackendSetupRequest {
    application::rust_janus_ga::RustJanusGaBackendSetupRequest {
        workdir: args.workdir.clone(),
        workers: args.workers,
        keep_dirs: args.keep_dirs,
        timeout_secs: args.timeout_secs,
        python_bin: args.python_bin.clone(),
        janus_adapter_script: args.janus_adapter_script.clone(),
        janus_arch: args.janus_arch.clone(),
        janus_model: args.janus_model.clone(),
        janus_device: args.janus_device.clone(),
        janus_dtype: args.janus_dtype.clone(),
        janus_mode: args.janus_mode,
        janus_optimizer: args.janus_optimizer,
        janus_fmax: args.janus_fmax,
        janus_steps: args.janus_steps,
    }
}

fn staged_runtime_backend_config_from_staged_ga_args(
    args: &RunScottStagedGaArgs,
) -> application::local_staged_runtime::StagedRuntimeBackendConfig {
    application::local_staged_runtime::StagedRuntimeBackendConfig {
        timeout: args.timeout_secs.map(Duration::from_secs),
        scott_input_dir: args.scott_input_dir.clone(),
        executable: args.executable.clone(),
        master_gin_template: args.master_gin_template.clone(),
        run_job_template: args.run_job_template.clone(),
        atoms_in_template: args.atoms_in_template.clone(),
        runtime_default_backend: args
            .runtime_default_backend
            .clone()
            .unwrap_or_else(|| "gulp".to_string()),
        runtime_stage_backend: args.runtime_stage_backend.clone(),
        python_bin: args.python_bin.clone(),
        janus_adapter_script: args.janus_adapter_script.clone(),
        janus_arch: args.janus_arch.clone(),
        janus_model: args.janus_model.clone(),
        janus_device: args.janus_device.clone(),
        janus_dtype: args.janus_dtype.clone(),
        janus_mode: args.janus_mode.into(),
        janus_optimizer: args.janus_optimizer.into(),
        janus_fmax: args.janus_fmax,
        janus_steps: args.janus_steps,
    }
}

fn staged_scott_ga_artifact_metadata_from_args(
    args: &RunScottStagedGaArgs,
) -> application::rust_janus_ga_artifacts::StagedScottGaArtifactMetadata {
    application::rust_janus_ga_artifacts::StagedScottGaArtifactMetadata {
        run_dir: args.run_dir.clone(),
        workdir: args.workdir.clone(),
        system: args.system.clone(),
        ga_generations: args.ga_generations,
        population: args.population,
        temperature: args.temperature,
        step_size: args.step_size,
        runtime_default_backend: args
            .runtime_default_backend
            .clone()
            .unwrap_or_else(|| "gulp".to_string()),
        runtime_stage_backend: args.runtime_stage_backend.clone(),
        run_job_template: args.run_job_template.clone(),
        master_gin_template: args.master_gin_template.clone(),
        atoms_in_template: args.atoms_in_template.clone(),
    }
}

fn staged_runtime_backend_config_from_eval_args(
    args: &EvaluateScottStagedArgs,
) -> application::local_staged_runtime::StagedRuntimeBackendConfig {
    application::local_staged_runtime::StagedRuntimeBackendConfig {
        timeout: args.timeout_secs.map(Duration::from_secs),
        scott_input_dir: args.scott_input_dir.clone(),
        executable: args.executable.clone(),
        master_gin_template: args.master_gin_template.clone(),
        run_job_template: args.run_job_template.clone(),
        atoms_in_template: args.atoms_in_template.clone(),
        runtime_default_backend: args.runtime_default_backend.clone(),
        runtime_stage_backend: args.runtime_stage_backend.clone(),
        python_bin: args.python_bin.clone(),
        janus_adapter_script: args.janus_adapter_script.clone(),
        janus_arch: args.janus_arch.clone(),
        janus_model: args.janus_model.clone(),
        janus_device: args.janus_device.clone(),
        janus_dtype: args.janus_dtype.clone(),
        janus_mode: args.janus_mode.into(),
        janus_optimizer: args.janus_optimizer.into(),
        janus_fmax: args.janus_fmax,
        janus_steps: args.janus_steps,
    }
}

fn staged_runtime_backend_config_from_production_args(
    args: &RunScottProductionArgs,
    timeout: Option<Duration>,
) -> application::local_staged_runtime::StagedRuntimeBackendConfig {
    application::local_staged_runtime::StagedRuntimeBackendConfig {
        timeout,
        scott_input_dir: args.scott_input_dir.clone(),
        executable: args.executable.clone(),
        master_gin_template: args.master_gin_template.clone(),
        run_job_template: args.run_job_template.clone(),
        atoms_in_template: args.atoms_in_template.clone(),
        runtime_default_backend: args.runtime_default_backend.clone(),
        runtime_stage_backend: args.runtime_stage_backend.clone(),
        python_bin: args.python_bin.clone(),
        janus_adapter_script: args.janus_adapter_script.clone(),
        janus_arch: args.janus_arch.clone(),
        janus_model: args.janus_model.clone(),
        janus_device: args.janus_device.clone(),
        janus_dtype: args.janus_dtype.clone(),
        janus_mode: args.janus_mode.into(),
        janus_optimizer: args.janus_optimizer.into(),
        janus_fmax: args.janus_fmax,
        janus_steps: args.janus_steps,
    }
}

fn evaluate_scott_staged(args: EvaluateScottStagedArgs) -> Result<()> {
    let candidate = read_candidate_json(&args.candidate_json)?;
    fs::create_dir_all(&args.workdir).with_context(|| {
        format!(
            "failed to create staged evaluation workdir `{}`",
            args.workdir.display()
        )
    })?;

    let (executor, setup, utf8_workdir) =
        application::local_staged_runtime::build_staged_runtime_setup(
            &application::local_staged_runtime::StagedRuntimeRequest {
                workdir: args.workdir.clone(),
                candidate: candidate.clone(),
                procedure_intent: ScottProcedureIntent::SingleEvaluation,
                backend: staged_runtime_backend_config_from_eval_args(&args),
            },
        )?;
    let request = patina_evaluator::ScottProcedureRequest {
        candidate: candidate.clone(),
        request_id: format!("staged-{}", candidate.label),
        workdir: utf8_workdir,
    };

    let outcome = run_local_procedure(&setup.procedure_plan, &request, &executor)
        .context("staged Scott runtime evaluation failed")?;

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "routing_policy": setup.routing_policy,
            "outcome": outcome,
        }))
        .context("failed to serialize staged Scott outcome")?
    );

    if let Some(run_dir) = args.run_dir.as_ref() {
        let procedure_digest = outcome.state_digest();
        let final_result = outcome.final_result.as_ref();
        let failure_reason = final_result.is_none().then(|| {
            outcome
                .last_failure_message()
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| {
                    "staged Scott procedure produced no accepted final result".into()
                })
        });
        application::single_eval_artifacts::write_single_eval_artifacts(
            run_dir,
            &application::single_eval_artifacts::SingleEvalArtifactArgs {
                workdir: args.workdir.clone(),
                mode: args.mode,
                system: args.system,
                engine: format!("Scott staged ({:?})", setup.default_backend),
                backend: if setup.default_backend == ScottBackendMode::JanusMace {
                    EvalBackendKind::JanusMace
                } else {
                    EvalBackendKind::Gulp
                }
                .into(),
                backend_metadata: Some(serde_json::json!({
                    "routing_policy": setup.routing_policy,
                    "procedure_plan": setup.procedure_plan,
                    "procedure_outcome": outcome,
                    "procedure_state_digest": procedure_digest.clone(),
                })),
                procedure_state_digest: Some(procedure_digest),
                failure_reason,
            },
            &candidate,
            final_result,
        )?;
    }

    Ok(())
}

fn run_hybrid_ga_production(args: RunHybridGaProductionArgs) -> Result<()> {
    fs::create_dir_all(&args.workdir)
        .with_context(|| format!("failed to create workdir `{}`", args.workdir.display()))?;
    fs::create_dir_all(&args.run_dir)
        .with_context(|| format!("failed to create run dir `{}`", args.run_dir.display()))?;

    let ga_workdir = args.workdir.join("ga");
    let production_workdir = args.workdir.join("production");
    let ga_run_dir = args.run_dir.join("ga");
    let production_run_dir = args.run_dir.join("production");
    let emulate_output_dir = args.run_dir.join("emulate");

    fs::create_dir_all(&ga_workdir)
        .with_context(|| format!("failed to create GA workdir `{}`", ga_workdir.display()))?;
    fs::create_dir_all(&production_workdir).with_context(|| {
        format!(
            "failed to create production workdir `{}`",
            production_workdir.display()
        )
    })?;
    fs::create_dir_all(&ga_run_dir)
        .with_context(|| format!("failed to create GA run dir `{}`", ga_run_dir.display()))?;
    fs::create_dir_all(&production_run_dir).with_context(|| {
        format!(
            "failed to create production run dir `{}`",
            production_run_dir.display()
        )
    })?;

    let ga_args = hybrid_args_as_staged_ga_args(&args, ga_workdir.clone(), ga_run_dir.clone());
    let production_args = hybrid_args_as_production_args(
        &args,
        production_workdir.clone(),
        production_run_dir.clone(),
    );

    let ga_rust_args = staged_ga_args_as_rust_janus_args(&ga_args);
    let core_request = rust_janus_ga_core_request_from_args(&ga_rust_args)?;
    let core = application::rust_janus_ga::prepare_rust_janus_ga_core(&core_request)?;
    let duplicate_trace = core.evidence_policy.duplicate_trace.clone();
    let search_cfg = core.controller_bootstrap.search_cfg.clone();
    let operator_policy = core.lane_metadata.operator_policy.clone();
    let lane_mode = core.lane_metadata.lane_mode;
    let duplicate_policy_mode = core.evidence_policy.duplicate_policy_mode;
    let duplicate_filter_stack = core.evidence_policy.duplicate_filter_stack.clone();
    let (executor, routing_policy, procedure_plan, utf8_workdir) =
        application::local_staged_runtime::build_staged_ga_runtime_setup(
            &application::local_staged_runtime::StagedGaRuntimeRequest {
                workdir: ga_args.workdir.clone(),
                keep_dirs: ga_args.keep_dirs,
                backend: staged_runtime_backend_config_from_staged_ga_args(&ga_args),
            },
        )?;
    let evaluator = application::scott_ga_runtime::StagedScottGaEvaluator::new(
        executor,
        procedure_plan.clone(),
        utf8_workdir.join("ga_stage_runtime"),
    );
    let workflow_service = application::ga_workflow::GaWorkflowService::new(
        application::workflow_policy::GaWorkflowPolicy::scott_staged_runtime(),
    );
    let workflow_policy = workflow_service.policy();
    let ga_hashkey_config = build_ga_artifact_hashkey_config(
        args.use_dreadnaut_keys,
        &args.hashkey_radius,
        args.hashkey_radius_const,
        ga_run_dir.join("raw").join("hashkey_identity_runtime"),
    )?;
    let backend_mode_label = match routing_policy.default_backend {
        patina_evaluator::ScottBackendMode::Gulp => "gulp",
        patina_evaluator::ScottBackendMode::JanusMace => "janus_mace",
    };
    let incremental_artifact_sink =
        application::rust_janus_ga_artifacts::StagedScottIncrementalArtifactSink {
            run_dir: &ga_run_dir,
            system: &args.system,
            search_cfg: &search_cfg,
            requested_generations: args.ga_generations,
            operator_policy: &operator_policy,
            backend_mode_label,
            hashkey_config: ga_hashkey_config.as_ref(),
            duplicate_trace: &duplicate_trace,
            procedure_traces: evaluator.shared_procedure_traces(),
        };
    let ga_stage_adapter =
        application::hybrid_ga_production::ServiceBackedHybridGaStageAdapter::new(
            workflow_service,
            core,
            application::hybrid_ga_production::ServiceBackedHybridGaStageConfig {
                system: args.system.clone(),
                workdir: ga_workdir.clone(),
                requested_generations: args.ga_generations,
            },
            &evaluator,
            Some(&incremental_artifact_sink),
        );

    let timeout = args.timeout_secs.map(Duration::from_secs);
    let resolved_inputs = application::local_staged_runtime::resolve_staged_scott_inputs(
        production_args.scott_input_dir.as_deref(),
        production_args.master_gin_template.clone(),
        production_args.run_job_template.clone(),
        production_args.atoms_in_template.clone(),
        "hybrid staged production",
    )?;
    let run_job_text =
        fs::read_to_string(&resolved_inputs.run_job_template).with_context(|| {
            format!(
                "failed to read hybrid staged production run.job `{}`",
                resolved_inputs.run_job_template.display()
            )
        })?;
    let production_cfg =
        application::scott_production::ProductionRunConfig::from_run_job_text(&run_job_text);
    let atom_specs = resolved_inputs
        .atoms_in_template
        .as_deref()
        .map(application::scott_topology_export::parse_atoms_file)
        .transpose()?;
    let topology_hkg_path = if production_cfg.use_top_analysis {
        Some(match production_cfg.hkg_path.as_deref() {
            Some(path) => absolutize_path(path)?,
            None => patina_dreadnaut::bundled_dreadnaut_path(),
        })
    } else {
        None
    };
    let hashkey_runtime_dir = production_workdir.join("hashkey_runtime");
    if production_cfg.use_top_analysis && topology_hkg_path.is_some() {
        fs::create_dir_all(&hashkey_runtime_dir).with_context(|| {
            format!(
                "failed to create hybrid production hashkey runtime dir `{}`",
                hashkey_runtime_dir.display()
            )
        })?;
    }

    let evaluation_port = application::local_staged_runtime::LocalStagedProductionPort {
        config: application::local_staged_runtime::LocalStagedProductionPortConfig {
            runtime: staged_runtime_backend_config_from_production_args(&production_args, timeout),
            production_cfg: production_cfg.clone(),
            atom_specs: atom_specs.clone(),
            topology_hkg_path: topology_hkg_path.clone(),
            hashkey_runtime_dir: hashkey_runtime_dir.clone(),
        },
    };
    let progress_port =
        application::local_staged_runtime::LocalProductionProgressPort { restart_dir: None };
    let production_artifact_sink = application::local_staged_runtime::LocalProductionArtifactSink {
        run_dir: Some(production_run_dir.clone()),
    };
    let production_stage_adapter =
        application::hybrid_ga_production::ServiceBackedHybridProductionStageAdapter::new(
            application::scott_production::ProductionWorkflowService,
            &evaluation_port,
            &evaluation_port,
            &progress_port,
            &production_artifact_sink,
        );

    let stage_overrides =
        application::driver_support::parse_runtime_stage_overrides(&args.runtime_stage_backend)?;
    let mut fallback_routing_policy = ScottBackendRoutingPolicy {
        default_backend: application::driver_support::parse_scott_backend_mode_label(
            &args.runtime_default_backend,
        )?,
        stage_overrides: IndexMap::new(),
    };
    for (stage, backend) in stage_overrides {
        fallback_routing_policy = fallback_routing_policy.with_stage_backend(
            stage,
            application::driver_support::parse_scott_backend_mode_label(&backend)?,
        );
    }
    let seed_selection_mode: application::hybrid_ga_production::HybridSeedSelectionMode =
        args.seed_selection_mode.into();
    let hybrid_request = application::hybrid_ga_production::HybridGaProductionRequest {
        system: args.system.clone(),
        workdir: args.workdir.clone(),
        max_production_seeds: args.max_production_seeds,
        seed_selection_mode,
        seed_candidate_mode: args.seed_candidate_mode.into(),
        crossover_config: patina_types::HybridCrossoverConfig {
            scientific_mode: args.hybrid_scientific_mode.into(),
            attempt_count: args.hybrid_crossover_attempts,
        },
        emulate_context: matches!(
            seed_selection_mode,
            application::hybrid_ga_production::HybridSeedSelectionMode::AcquisitionGuided
        )
        .then(|| application::hybrid_ga_production::HybridEmulateContext {
            campaign_id: patina_emulate::CampaignId(
                args.emulate_campaign_id.clone().unwrap_or_else(|| {
                    format!(
                        "campaign-{}",
                        args.run_dir
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("hybrid-run")
                    )
                }),
            ),
            branch_id: patina_emulate::BranchId(args.emulate_branch_id.clone()),
            fidelity: args.emulate_fidelity.into(),
        }),
        require_converged_ga_seeds: args.require_converged_ga_seeds,
        seed: args.seed.unwrap_or(0),
        production_config: production_cfg.clone(),
        restart_state_before: None,
        restart_counter_base: 0,
        fallback_routing_policy: fallback_routing_policy.clone(),
    };

    let influence_mode = match seed_selection_mode {
        application::hybrid_ga_production::HybridSeedSelectionMode::TopRanked => {
            "top_ranked_production_promotion"
        }
        application::hybrid_ga_production::HybridSeedSelectionMode::AcquisitionGuided => {
            "emulate_acquisition_guided_promotion"
        }
    };
    let hybrid_artifact_sink =
        application::local_staged_runtime::LocalHybridGaProductionArtifactSink {
            run_dir: args.run_dir.clone(),
            influence_mode,
            emulate_output_dir: matches!(
                seed_selection_mode,
                application::hybrid_ga_production::HybridSeedSelectionMode::AcquisitionGuided
            )
            .then_some(emulate_output_dir.clone()),
        };
    let hybrid_service = application::hybrid_ga_production::HybridGaProductionWorkflowService;

    let execution = match seed_selection_mode {
        application::hybrid_ga_production::HybridSeedSelectionMode::TopRanked => {
            let seed_policy = application::hybrid_ga_production::TopRankedHybridSeedSelectionPolicy;
            hybrid_service.execute(
                &hybrid_request,
                &ga_stage_adapter,
                &production_stage_adapter,
                &seed_policy,
                &hybrid_artifact_sink,
            )?
        }
        application::hybrid_ga_production::HybridSeedSelectionMode::AcquisitionGuided => {
            fs::create_dir_all(&emulate_output_dir).with_context(|| {
                format!(
                    "failed to create emulate output dir `{}`",
                    emulate_output_dir.display()
                )
            })?;
            let projector = build_emulate_feature_projector(
                args.emulate_feature_projector,
                args.emulate_uv_bin.as_deref(),
                args.emulate_project_dir.as_deref(),
                &args.emulate_environment,
            )?;
            let surrogate = patina_emulate::UvSurrogateRuntimeAdapter::new(
                patina_emulate::UvSurrogateRuntimeConfig {
                    uv_bin: args
                        .emulate_uv_bin
                        .clone()
                        .unwrap_or_else(|| PathBuf::from("uv")),
                    project_dir: absolutize_path(
                        args.emulate_project_dir
                            .as_deref()
                            .unwrap_or_else(|| Path::new("crates/patina-emulate/python")),
                    )?,
                    work_root: emulate_output_dir.clone(),
                    environment_name: args.emulate_environment.clone(),
                    extras: vec!["autoemulate".into()],
                    native_tls: true,
                    timeout: Some(Duration::from_secs(args.emulate_timeout_secs)),
                    checkpointing: patina_emulate::RuntimeCheckpointConfig::enabled(),
                },
            );
            let seed_policy =
                application::hybrid_ga_production::EmulateBackedHybridSeedSelectionAdapter::new(
                    application::hybrid_ga_production::EmulateBackedHybridSeedSelectionConfig {
                        objective: args.emulate_objective.into(),
                        surrogate: patina_emulate::SurrogateConfig {
                            model_variant: args.emulate_model_variant.into(),
                            epochs: args.emulate_epochs,
                            num_inducing: args.emulate_num_inducing,
                            batch_size: args.emulate_batch_size,
                            lr: args.emulate_lr,
                            n_bootstraps: args.emulate_n_bootstraps,
                            random_seed: args.emulate_random_seed,
                            log_level: args.emulate_log_level.clone(),
                            objective: args.emulate_surrogate_objective.into(),
                            primary_target_index: 0,
                            checkpoint: None,
                        },
                        target_name: args.emulate_target_name.clone(),
                        target_unit: Some(args.emulate_target_unit.clone()),
                        provenance_label: Some("patina_driver.hybrid_emulate_selection".into()),
                    },
                    &*projector,
                    &surrogate,
                );
            hybrid_service.execute(
                &hybrid_request,
                &ga_stage_adapter,
                &production_stage_adapter,
                &seed_policy,
                &hybrid_artifact_sink,
            )?
        }
    };

    let procedure_traces = evaluator.procedure_traces();
    let duplicate_traces = application::rust_janus_ga::snapshot_duplicate_trace(&duplicate_trace);
    let artifact_metadata = staged_scott_ga_artifact_metadata_from_args(&ga_args);
    let ga_artifact_sink = application::rust_janus_ga_artifacts::StagedScottGaArtifactSink {
        metadata: &artifact_metadata,
        base_candidate_source_path: core_request.base_candidate_source_path.as_deref(),
        search_cfg: &search_cfg,
        operator_policy: &operator_policy,
        hashkey_config: ga_hashkey_config.as_ref(),
        lane_mode,
        duplicate_policy_mode,
        duplicate_filter_stack: &duplicate_filter_stack,
        duplicate_traces: &duplicate_traces,
        routing_policy: &routing_policy,
        procedure_plan: &procedure_plan,
        procedure_traces: &procedure_traces,
        emulate_gate_summary: None,
        emulate_gate_decision_traces: None,
        emulate_gate_batch_traces: None,
        workflow_policy,
    };
    let _ga_summary = application::ports::GaRunArtifactSink::persist_run(
        &ga_artifact_sink,
        &execution.ga_execution,
    )?;
    application::scott_production::write_staged_production_artifacts(
        &production_run_dir,
        &execution.production_execution.summary,
        &execution.production_execution.best_entries,
    )?;

    let emulate_artifact_dir = match seed_selection_mode {
        application::hybrid_ga_production::HybridSeedSelectionMode::TopRanked => None,
        application::hybrid_ga_production::HybridSeedSelectionMode::AcquisitionGuided => {
            application::driver_support::latest_surrogate_artifact_dir(&emulate_output_dir)?
        }
    };
    let summary = application::local_staged_runtime::HybridGaProductionSummaryReport {
        workflow_owner: "hybrid_ga_production",
        influence_mode,
        system: execution.summary.system.clone(),
        run_dir: args.run_dir.clone(),
        ga_run_dir,
        production_run_dir,
        emulate_artifact_dir,
        ga_generation_count: execution.ga_execution.generation_artifacts.len(),
        ga_population_size: execution.summary.ga_population_size,
        scientific_mode: execution.summary.scientific_mode,
        selected_seed_count: execution.summary.selected_seed_count,
        selected_seed_labels: execution
            .selected_seeds
            .iter()
            .map(|seed| seed.selected_candidate_label.clone())
            .collect(),
        accepted_child_count: execution.summary.accepted_child_count,
        accepted_child_labels: execution
            .accepted_children
            .iter()
            .map(|child| child.child_candidate_label.clone())
            .collect(),
        production_input_count: execution.summary.production_input_count,
        production_success_count: execution.summary.production_success_count,
        production_failure_count: execution.summary.production_failure_count,
        best_set_size: execution.summary.best_set_size,
    };

    println!(
        "{}",
        serde_json::to_string_pretty(&summary)
            .context("failed to serialize hybrid GA production summary")?
    );

    Ok(())
}

fn run_scott_production(args: RunScottProductionArgs) -> Result<()> {
    fs::create_dir_all(&args.workdir).with_context(|| {
        format!(
            "failed to create staged production workdir `{}`",
            args.workdir.display()
        )
    })?;

    let timeout = args.timeout_secs.map(Duration::from_secs);
    let resolved_inputs = application::local_staged_runtime::resolve_staged_scott_inputs(
        args.scott_input_dir.as_deref(),
        args.master_gin_template.clone(),
        args.run_job_template.clone(),
        args.atoms_in_template.clone(),
        "staged Scott production",
    )?;
    let run_job_text =
        fs::read_to_string(&resolved_inputs.run_job_template).with_context(|| {
            format!(
                "failed to read staged Scott production run.job `{}`",
                resolved_inputs.run_job_template.display()
            )
        })?;
    let production_cfg =
        application::scott_production::ProductionRunConfig::from_run_job_text(&run_job_text);
    let atom_specs = resolved_inputs
        .atoms_in_template
        .as_deref()
        .map(application::scott_topology_export::parse_atoms_file)
        .transpose()?;
    let production_inputs = load_production_seed_inputs(
        &args.candidate_json,
        args.restart_dir.as_deref(),
        &production_cfg,
        &resolved_inputs.master_gin_template,
        atom_specs.as_deref(),
    )?;
    let topology_hkg_path = if production_cfg.use_top_analysis {
        Some(match production_cfg.hkg_path.as_deref() {
            Some(path) => absolutize_path(path)?,
            None => patina_dreadnaut::bundled_dreadnaut_path(),
        })
    } else {
        None
    };
    let hashkey_runtime_dir = args.workdir.join("hashkey_runtime");
    if production_cfg.use_top_analysis && topology_hkg_path.is_some() {
        fs::create_dir_all(&hashkey_runtime_dir).with_context(|| {
            format!(
                "failed to create production hashkey runtime dir `{}`",
                hashkey_runtime_dir.display()
            )
        })?;
    }

    let restart_dir = args
        .restart_dir
        .as_deref()
        .map(absolutize_path)
        .transpose()?;
    let restart_state_before = restart_dir
        .as_deref()
        .map(application::scott_production::read_restart_state)
        .transpose()?
        .flatten();
    let restart_counter_base = restart_state_before
        .as_ref()
        .and_then(|state| state.counter)
        .unwrap_or(0);
    let evaluation_port = application::local_staged_runtime::LocalStagedProductionPort {
        config: application::local_staged_runtime::LocalStagedProductionPortConfig {
            runtime: staged_runtime_backend_config_from_production_args(&args, timeout),
            production_cfg: production_cfg.clone(),
            atom_specs: atom_specs.clone(),
            topology_hkg_path: topology_hkg_path.clone(),
            hashkey_runtime_dir: hashkey_runtime_dir.clone(),
        },
    };
    let progress_port = application::local_staged_runtime::LocalProductionProgressPort {
        restart_dir: restart_dir.clone(),
    };
    let artifact_sink = application::local_staged_runtime::LocalProductionArtifactSink {
        run_dir: args.run_dir.clone(),
    };
    let execution = application::scott_production::ProductionWorkflowService.execute(
        &production_inputs,
        &args.system,
        &args.workdir,
        &production_cfg,
        restart_state_before,
        restart_counter_base,
        ScottBackendRoutingPolicy {
            default_backend: application::driver_support::parse_scott_backend_mode_label(
                &args.runtime_default_backend,
            )?,
            stage_overrides: IndexMap::new(),
        },
        &evaluation_port,
        &evaluation_port,
        &progress_port,
        &artifact_sink,
    )?;
    let summary = execution.summary;

    if let Some(run_dir) = args.run_dir.as_ref() {
        application::scott_production::write_staged_production_artifacts(
            run_dir,
            &summary,
            &execution.best_entries,
        )?;
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&summary)
            .context("failed to serialize staged production summary")?
    );

    Ok(())
}

fn evaluate_backend_batch(args: EvaluateBackendBatchArgs) -> Result<()> {
    ensure_runner_mode_supported(args.backend, args.runner_mode)?;
    fs::create_dir_all(&args.workdir).with_context(|| {
        format!(
            "failed to create batch evaluation workdir `{}`",
            args.workdir.display()
        )
    })?;

    let summary = application::backend_runs::run_backend_batch(
        args.backend,
        args.runner_mode,
        &args.candidate_json,
        &args.workdir,
        args.workers,
        args.keep_dirs,
        args.timeout_secs,
        args.executable.clone(),
        args.master_gin_template.clone(),
        args.run_job_template.clone(),
        args.atoms_in_template.clone(),
        args.jobs_template.clone(),
        args.python_bin.clone(),
        args.janus_adapter_script.clone(),
        args.janus_arch.clone(),
        args.janus_model.clone(),
        args.janus_device.clone(),
        args.janus_dtype.clone(),
        args.janus_mode,
        args.janus_optimizer,
        args.janus_fmax,
        args.janus_steps,
    )?;
    println!(
        "{}",
        serde_json::to_string_pretty(&summary)
            .context("failed to serialize batch evaluation summary")?
    );

    Ok(())
}

fn compare_backend_batch(args: CompareBackendBatchArgs) -> Result<()> {
    fs::create_dir_all(&args.workdir).with_context(|| {
        format!(
            "failed to create batch comparison workdir `{}`",
            args.workdir.display()
        )
    })?;

    let transient_workdir = args.workdir.join("transient");
    let persistent_workdir = args.workdir.join("persistent");

    let transient = application::backend_runs::run_backend_batch(
        args.backend,
        RunnerMode::Transient,
        &args.candidate_json,
        &transient_workdir,
        args.workers,
        args.keep_dirs,
        args.timeout_secs,
        args.executable.clone(),
        args.master_gin_template.clone(),
        args.run_job_template.clone(),
        args.atoms_in_template.clone(),
        args.jobs_template.clone(),
        args.python_bin.clone(),
        args.janus_adapter_script.clone(),
        args.janus_arch.clone(),
        args.janus_model.clone(),
        args.janus_device.clone(),
        args.janus_dtype.clone(),
        args.janus_mode,
        args.janus_optimizer,
        args.janus_fmax,
        args.janus_steps,
    )?;

    let persistent = application::backend_runs::run_backend_batch(
        args.backend,
        RunnerMode::Persistent,
        &args.candidate_json,
        &persistent_workdir,
        args.workers,
        args.keep_dirs,
        args.timeout_secs,
        args.executable.clone(),
        args.master_gin_template.clone(),
        args.run_job_template.clone(),
        args.atoms_in_template.clone(),
        args.jobs_template.clone(),
        args.python_bin.clone(),
        args.janus_adapter_script.clone(),
        args.janus_arch.clone(),
        args.janus_model.clone(),
        args.janus_device.clone(),
        args.janus_dtype.clone(),
        args.janus_mode,
        args.janus_optimizer,
        args.janus_fmax,
        args.janus_steps,
    )?;

    let speedup = if persistent.telemetry.elapsed_secs > 0.0 {
        transient.telemetry.elapsed_secs / persistent.telemetry.elapsed_secs
    } else {
        0.0
    };

    let summary = serde_json::json!({
        "backend": args.backend,
        "candidate_count": args.candidate_json.len(),
        "workers": args.workers.unwrap_or_else(default_worker_count),
        "keep_dirs": args.keep_dirs,
        "transient": transient,
        "persistent": persistent,
        "comparison": {
            "elapsed_speedup": speedup,
            "elapsed_delta_secs": transient.telemetry.elapsed_secs - persistent.telemetry.elapsed_secs,
            "success_delta": transient.telemetry.success_count as i64 - persistent.telemetry.success_count as i64,
            "failure_delta": transient.telemetry.failure_count as i64 - persistent.telemetry.failure_count as i64,
        }
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&summary)
            .context("failed to serialize batch comparison summary")?
    );

    Ok(())
}

fn compare_backend_generations(args: CompareBackendGenerationsArgs) -> Result<()> {
    if args.generations == 0 {
        return Err(anyhow!("`--generations` must be at least 1"));
    }

    fs::create_dir_all(&args.workdir).with_context(|| {
        format!(
            "failed to create generation comparison workdir `{}`",
            args.workdir.display()
        )
    })?;

    let transient_workdir = args.workdir.join("transient");
    let persistent_workdir = args.workdir.join("persistent");

    let transient = application::backend_runs::run_backend_generations(
        args.backend,
        RunnerMode::Transient,
        &args.candidate_json,
        args.generations,
        &transient_workdir,
        args.workers,
        args.keep_dirs,
        args.timeout_secs,
        args.executable.clone(),
        args.master_gin_template.clone(),
        args.run_job_template.clone(),
        args.atoms_in_template.clone(),
        args.jobs_template.clone(),
        args.python_bin.clone(),
        args.janus_adapter_script.clone(),
        args.janus_arch.clone(),
        args.janus_model.clone(),
        args.janus_device.clone(),
        args.janus_dtype.clone(),
        args.janus_mode,
        args.janus_optimizer,
        args.janus_fmax,
        args.janus_steps,
    )?;

    let persistent = application::backend_runs::run_backend_generations(
        args.backend,
        RunnerMode::Persistent,
        &args.candidate_json,
        args.generations,
        &persistent_workdir,
        args.workers,
        args.keep_dirs,
        args.timeout_secs,
        args.executable.clone(),
        args.master_gin_template.clone(),
        args.run_job_template.clone(),
        args.atoms_in_template.clone(),
        args.jobs_template.clone(),
        args.python_bin.clone(),
        args.janus_adapter_script.clone(),
        args.janus_arch.clone(),
        args.janus_model.clone(),
        args.janus_device.clone(),
        args.janus_dtype.clone(),
        args.janus_mode,
        args.janus_optimizer,
        args.janus_fmax,
        args.janus_steps,
    )?;

    let speedup = if persistent.telemetry.elapsed_secs > 0.0 {
        transient.telemetry.elapsed_secs / persistent.telemetry.elapsed_secs
    } else {
        0.0
    };

    let summary = serde_json::json!({
        "backend": args.backend,
        "generations": args.generations,
        "candidate_count": args.candidate_json.len(),
        "workers": args.workers.unwrap_or_else(default_worker_count),
        "keep_dirs": args.keep_dirs,
        "transient": transient,
        "persistent": persistent,
        "comparison": {
            "elapsed_speedup": speedup,
            "elapsed_delta_secs": transient.telemetry.elapsed_secs - persistent.telemetry.elapsed_secs,
            "success_delta": transient.telemetry.success_count as i64 - persistent.telemetry.success_count as i64,
            "failure_delta": transient.telemetry.failure_count as i64 - persistent.telemetry.failure_count as i64,
        },
        "scheduling_contract": {
            "generation_control": "serial",
            "candidate_evaluation_within_generation": "parallel",
            "worker_count": args.workers.unwrap_or_else(default_worker_count),
            "workers_used_per_generation": format!("min(population_or_batch_size, {})", args.workers.unwrap_or_else(default_worker_count)),
            "generation_barrier": "all evaluations in a generation complete before the next generation begins",
            "ga_logic_scope": "this command benchmarks evaluation scheduling only; it does not model selection, crossover, mutation, reinsertion, or duplicate policy"
        }
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&summary)
            .context("failed to serialize generation comparison summary")?
    );

    Ok(())
}

fn run_backend_campaign(args: RunBackendCampaignArgs) -> Result<()> {
    ensure_runner_mode_supported(args.backend, args.runner_mode)?;
    if args.generations == 0 {
        return Err(anyhow!("`--generations` must be at least 1"));
    }
    if args.generations > 1 {
        eprintln!(
            "note: `run-backend-campaign` currently replays the same base candidate pack for each generation; it does not implement GA/BH propagation"
        );
    }

    fs::create_dir_all(&args.workdir).with_context(|| {
        format!(
            "failed to create campaign workdir `{}`",
            args.workdir.display()
        )
    })?;
    fs::create_dir_all(&args.run_dir).with_context(|| {
        format!(
            "failed to create campaign run dir `{}`",
            args.run_dir.display()
        )
    })?;

    let result = application::backend_runs::execute_backend_campaign(
        args.backend,
        args.runner_mode,
        &args.candidate_json,
        args.generations,
        &args.workdir,
        args.workers,
        args.keep_dirs,
        args.timeout_secs,
        args.executable,
        args.master_gin_template,
        args.run_job_template,
        args.atoms_in_template.clone(),
        args.jobs_template,
        args.python_bin,
        args.janus_adapter_script,
        args.janus_arch,
        args.janus_model,
        args.janus_device,
        args.janus_dtype,
        args.janus_mode,
        args.janus_optimizer,
        args.janus_fmax,
        args.janus_steps,
    )?;

    application::backend_campaign_artifacts::write_backend_campaign_artifacts(
        &args.run_dir,
        &args.system,
        &args.candidate_json,
        args.backend,
        args.runner_mode,
        args.atoms_in_template.as_deref(),
        &result,
    )?;

    let summary = serde_json::json!({
        "run_dir": args.run_dir,
        "system": args.system,
        "backend": args.backend,
        "runner_mode": args.runner_mode,
        "workers": args.workers.unwrap_or_else(default_worker_count),
        "generations": args.generations,
        "candidate_count": args.candidate_json.len(),
        "campaign_summary": result.summary,
        "generation_artifacts": result.generations,
        "workflow_contract": {
            "owner": "rust_evaluator_campaign",
            "generation_control": "serial",
            "candidate_evaluation_within_generation": "parallel",
            "generation_barrier": "all evaluations in a generation complete before the next generation begins",
            "scope": "tracked evaluator campaign only; native SCOTT GA/BH semantics remain separate"
        }
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&summary)
            .context("failed to serialize backend campaign summary")?
    );

    Ok(())
}

fn run_rust_janus_search(args: RunRustJanusSearchArgs) -> Result<()> {
    let core_request = rust_janus_ga_core_request_from_args(&args)?;
    let backend_request = rust_janus_ga_backend_setup_request_from_args(&args);
    let setup =
        application::rust_janus_ga::prepare_rust_janus_ga_setup(&core_request, &backend_request)?;
    let started = std::time::Instant::now();
    let worker_session_dir = setup.pool.session_dir().to_path_buf();
    let search_cfg = setup.core.controller_bootstrap.search_cfg.clone();
    let python_bin = setup.python_bin.clone();
    let adapter_script = setup.adapter_script.clone();
    let lane_mode = setup.core.lane_metadata.lane_mode;
    let duplicate_policy_mode = setup.core.evidence_policy.duplicate_policy_mode;
    let duplicate_filter_stack = setup.core.evidence_policy.duplicate_filter_stack.clone();
    let operator_policy = setup.core.lane_metadata.operator_policy.clone();
    let hashkey_config = build_ga_artifact_hashkey_config(
        args.use_dreadnaut_keys,
        &args.hashkey_radius,
        args.hashkey_radius_const,
        args.run_dir.join("raw").join("hashkey_identity_runtime"),
    )?;
    let execution = application::rust_janus_ga_execution::execute_rust_janus_ga_search(
        setup,
        application::rust_janus_ga_execution::RustJanusGaExecutionRequest {
            requested_generations: args.ga_generations,
        },
        application::rust_janus_ga_artifacts::RustJanusIncrementalArtifactMetadata {
            run_dir: &args.run_dir,
            system: &args.system,
            search_cfg: &search_cfg,
            requested_generations: args.ga_generations,
            backend_mode_label: args.janus_mode.as_str(),
            operator_policy: &operator_policy,
            hashkey_config: hashkey_config.as_ref(),
        },
    )?;

    application::rust_janus_ga_artifacts::write_rust_janus_ga_artifacts(
        &application::rust_janus_ga_artifacts::RustJanusGaArtifactContext {
            run_dir: &args.run_dir,
            system: &args.system,
            base_candidate_source_path: core_request.base_candidate_source_path.as_deref(),
            search_cfg: &search_cfg,
            workers: args.workers.unwrap_or_else(default_worker_count),
            requested_generations: args.ga_generations,
            population_size: args.population,
            temperature: args.temperature,
            step_size: args.step_size,
            enable_pmoi: args.enable_pmoi,
            pmoi_tolerance: args.pmoi_tolerance,
            lane_mode,
            duplicate_policy_mode,
            duplicate_filter_stack: &duplicate_filter_stack,
            operator_policy: &operator_policy,
            hashkey_config: hashkey_config.as_ref(),
            backend: application::rust_janus_ga_artifacts::RustJanusGaBackendArtifactMetadata {
                python_bin: &python_bin,
                adapter_script: &adapter_script,
                arch: &args.janus_arch,
                model: &args.janus_model,
                device: &args.janus_device,
                dtype: &args.janus_dtype,
                mode_label: args.janus_mode.as_str(),
                optimizer_label: args.janus_optimizer.as_str(),
                fmax: args.janus_fmax,
                steps: args.janus_steps,
                worker_session_dir: &worker_session_dir,
            },
        },
        &execution.generation_artifacts,
        &execution.generation_responses,
        &execution.generation_origin_metrics,
        &execution.generation_boundary_populations,
        &execution.controller_trace,
        &execution.final_population,
        started.elapsed().as_secs_f64(),
    )?;

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "status": "ok",
            "workflow_owner": "persistent_daemon_ga",
            "run_dir": args.run_dir,
            "workdir": args.workdir,
            "generations_recorded": execution.generation_artifacts.len(),
            "workers": args.workers.unwrap_or_else(default_worker_count)
        }))
        .context("failed to serialize rust janus search summary")?
    );

    Ok(())
}

fn run_score_ga_emulate(args: ScoreGaEmulateArgs) -> Result<()> {
    let ga_run_dir = absolutize_path(&args.ga_run_dir)?;
    let output_dir = absolutize_path(&args.output_dir)?;
    fs::create_dir_all(&output_dir)
        .with_context(|| format!("failed to create output dir `{}`", output_dir.display()))?;

    let campaign_id = args.campaign_id.unwrap_or_else(|| {
        format!(
            "campaign-{}",
            ga_run_dir
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("ga-run")
        )
    });
    let branch_id = args.branch_id;
    let objective: patina_emulate::LearningObjective = args.objective.into();
    let fidelity: patina_emulate::FidelityClass = args.fidelity.into();
    let observations = application::ga_emulate::load_ga_training_observations_from_run_dir(
        &patina_emulate::CampaignId(campaign_id.clone()),
        &patina_emulate::BranchId(branch_id.clone()),
        fidelity,
        &ga_run_dir,
    )?;
    let pending_paths = collect_pending_candidate_paths(
        &args.pending_candidate,
        args.pending_candidate_dir.as_deref(),
    )?;
    let pending_candidates = pending_paths
        .iter()
        .map(|path| load_candidate_from_snapshot_path(path))
        .collect::<Result<Vec<_>>>()?;

    let projector = build_emulate_feature_projector(
        args.feature_projector,
        args.uv_bin.as_deref(),
        args.emulate_project_dir.as_deref(),
        &args.emulate_environment,
    )?;
    let surrogate =
        patina_emulate::UvSurrogateRuntimeAdapter::new(patina_emulate::UvSurrogateRuntimeConfig {
            uv_bin: args.uv_bin.unwrap_or_else(|| PathBuf::from("uv")),
            project_dir: absolutize_path(
                args.emulate_project_dir
                    .as_deref()
                    .unwrap_or_else(|| Path::new("crates/patina-emulate/python")),
            )?,
            work_root: output_dir.clone(),
            environment_name: args.emulate_environment,
            extras: vec!["autoemulate".into()],
            native_tls: true,
            timeout: Some(Duration::from_secs(args.timeout_secs)),
            checkpointing: patina_emulate::RuntimeCheckpointConfig::enabled(),
        });
    let service = application::ga_emulate::GaEmulateScoringService;
    let execution = service.score_pending_candidates_from_observations(
        &application::ga_emulate::GaEmulateScoringRequest {
            campaign_id: patina_emulate::CampaignId(campaign_id.clone()),
            branch_id: patina_emulate::BranchId(branch_id.clone()),
            objective,
            fidelity,
            surrogate: patina_emulate::SurrogateConfig {
                model_variant: args.model_variant.into(),
                epochs: args.epochs,
                num_inducing: args.num_inducing,
                batch_size: args.batch_size,
                lr: args.lr,
                n_bootstraps: args.n_bootstraps,
                random_seed: args.random_seed,
                log_level: args.log_level,
                objective: args.surrogate_objective.into(),
                primary_target_index: 0,
                checkpoint: None,
            },
            target_name: args.target_name,
            target_unit: Some(args.target_unit),
            provenance_label: Some("patina_driver.ga_emulate_score".into()),
        },
        observations,
        &pending_candidates,
        &*projector,
        &surrogate,
    )?;

    let artifact_dir = application::driver_support::latest_surrogate_artifact_dir(&output_dir)?
        .unwrap_or_else(|| output_dir.clone());
    fs::write(
        artifact_dir.join("training_observations.json"),
        serde_json::to_string_pretty(&execution.observations)
            .context("failed to serialize training observations")?,
    )
    .with_context(|| {
        format!(
            "failed to write `{}`",
            artifact_dir.join("training_observations.json").display()
        )
    })?;
    fs::write(
        artifact_dir.join("acquisition_records.json"),
        serde_json::to_string_pretty(&execution.acquisitions)
            .context("failed to serialize acquisition records")?,
    )
    .with_context(|| {
        format!(
            "failed to write `{}`",
            artifact_dir.join("acquisition_records.json").display()
        )
    })?;

    let best_ranked = execution.acquisitions.first().cloned();
    let highest_uncertainty = execution
        .acquisitions
        .iter()
        .max_by(|left, right| left.uncertainty_score.total_cmp(&right.uncertainty_score))
        .cloned();
    let summary = application::report_types::GaEmulateSummaryReport {
        workflow_owner: "ga_emulate_score",
        influence_mode: "advisory_downstream_ranking",
        ga_run_dir: ga_run_dir.clone(),
        surrogate_artifact_dir: artifact_dir.clone(),
        campaign_id,
        branch_id,
        objective,
        fidelity,
        model_variant: execution.scoring_request.surrogate.model_variant,
        selected_model_name: execution.scoring_response.selected_model_name.clone(),
        feature_family: execution.feature_representation.family.clone(),
        feature_version: execution.feature_representation.version.clone(),
        feature_count: execution.feature_representation.feature_names.len(),
        training_observation_count: execution.observations.len(),
        pending_candidate_count: execution.scoring_request.candidate_rows.len(),
        incumbent_target: execution.scoring_response.incumbent_target,
        best_ranked_candidate_id: best_ranked
            .as_ref()
            .map(|record| record.candidate_id.clone()),
        highest_uncertainty_candidate_id: highest_uncertainty
            .as_ref()
            .map(|record| record.candidate_id.clone()),
        top_acquisition_score: best_ranked.as_ref().map(|record| record.acquisition_score),
        top_uncertainty_score: highest_uncertainty
            .as_ref()
            .map(|record| record.uncertainty_score),
    };
    fs::write(
        artifact_dir.join("emulate_summary.json"),
        serde_json::to_string_pretty(&summary).context("failed to serialize emulate summary")?,
    )
    .with_context(|| {
        format!(
            "failed to write `{}`",
            artifact_dir.join("emulate_summary.json").display()
        )
    })?;
    let report = build_ga_emulate_report(
        &ga_run_dir,
        &artifact_dir,
        &summary,
        &execution.scoring_request,
        &execution.scoring_response,
        &execution.observations,
    );
    fs::write(
        artifact_dir.join("emulate_report.json"),
        serde_json::to_string_pretty(&report).context("failed to serialize emulate report")?,
    )
    .with_context(|| {
        format!(
            "failed to write `{}`",
            artifact_dir.join("emulate_report.json").display()
        )
    })?;

    println!(
        "{}",
        serde_json::to_string_pretty(&summary).context("failed to serialize GA emulate summary")?
    );
    Ok(())
}

fn build_ga_emulate_report(
    ga_run_dir: &Path,
    artifact_dir: &Path,
    summary: &application::report_types::GaEmulateSummaryReport,
    scoring_request: &patina_emulate::SurrogateScoringRequest,
    scoring_response: &patina_emulate::SurrogateScoringResponse,
    observations: &[patina_emulate::CandidateObservation],
) -> application::report_types::GaEmulateReport {
    let candidate_labels = scoring_request
        .candidate_rows
        .iter()
        .map(|row| (row.candidate_id.clone(), row.metadata.get("label").cloned()))
        .collect::<BTreeMap<_, _>>();

    let mut diagnostic_rows = scoring_response
        .predictions
        .iter()
        .map(
            |prediction| application::report_types::GaEmulateCandidateDiagnosticRow {
                candidate_id: prediction.candidate_id.clone(),
                label: candidate_labels
                    .get(&prediction.candidate_id)
                    .cloned()
                    .flatten(),
                rank: prediction.rank,
                predicted_mean: prediction.means[0],
                predicted_variance: prediction.variances[0],
                acquisition_score: prediction.acquisition_score,
                uncertainty_score: prediction.uncertainty_score,
            },
        )
        .collect::<Vec<_>>();
    diagnostic_rows.sort_by_key(|row| row.rank);

    let mut highest_uncertainty_candidates = diagnostic_rows.clone();
    highest_uncertainty_candidates.sort_by(|left, right| {
        right
            .uncertainty_score
            .total_cmp(&left.uncertainty_score)
            .then_with(|| left.rank.cmp(&right.rank))
    });

    let training_targets = observations
        .iter()
        .filter_map(|observation| observation.evaluated_candidate.as_ref())
        .map(|evaluation| evaluation.energy);

    application::report_types::GaEmulateReport {
        workflow_owner: summary.workflow_owner,
        influence_mode: summary.influence_mode,
        ga_run_dir: ga_run_dir.to_path_buf(),
        surrogate_artifact_dir: artifact_dir.to_path_buf(),
        campaign_id: summary.campaign_id.clone(),
        branch_id: summary.branch_id.clone(),
        objective: summary.objective,
        fidelity: summary.fidelity,
        model_variant: summary.model_variant,
        selected_model_name: summary.selected_model_name.clone(),
        feature_family: summary.feature_family.clone(),
        feature_version: summary.feature_version.clone(),
        feature_count: summary.feature_count,
        training_observation_count: summary.training_observation_count,
        pending_candidate_count: summary.pending_candidate_count,
        incumbent_target: summary.incumbent_target,
        training_target_summary: summarize_scalar_metric(training_targets),
        predicted_target_summary: summarize_scalar_metric(
            diagnostic_rows.iter().map(|row| row.predicted_mean),
        ),
        predicted_variance_summary: summarize_scalar_metric(
            diagnostic_rows.iter().map(|row| row.predicted_variance),
        ),
        acquisition_score_summary: summarize_scalar_metric(
            diagnostic_rows.iter().map(|row| row.acquisition_score),
        ),
        uncertainty_score_summary: summarize_scalar_metric(
            diagnostic_rows.iter().map(|row| row.uncertainty_score),
        ),
        top_ranked_candidates: diagnostic_rows.iter().take(5).cloned().collect(),
        highest_uncertainty_candidates: highest_uncertainty_candidates
            .iter()
            .take(5)
            .cloned()
            .collect(),
    }
}

fn summarize_scalar_metric(
    values: impl IntoIterator<Item = f64>,
) -> Option<application::report_types::ScalarMetricSummary> {
    let mut count = 0usize;
    let mut sum = 0.0f64;
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;

    for value in values {
        if !value.is_finite() {
            continue;
        }
        count += 1;
        sum += value;
        min = min.min(value);
        max = max.max(value);
    }

    (count > 0).then_some(application::report_types::ScalarMetricSummary {
        min,
        max,
        mean: sum / count as f64,
    })
}

fn run_scott_staged_ga(args: RunScottStagedGaArgs) -> Result<()> {
    fs::create_dir_all(&args.workdir)
        .with_context(|| format!("failed to create workdir `{}`", args.workdir.display()))?;
    fs::create_dir_all(&args.run_dir)
        .with_context(|| format!("failed to create run dir `{}`", args.run_dir.display()))?;

    let ga_args = staged_ga_args_as_rust_janus_args(&args);
    let core_request = rust_janus_ga_core_request_from_args(&ga_args)?;
    let core = application::rust_janus_ga::prepare_rust_janus_ga_core(&core_request)?;
    let duplicate_trace = core.evidence_policy.duplicate_trace.clone();
    let (executor, routing_policy, procedure_plan, utf8_workdir) =
        application::local_staged_runtime::build_staged_ga_runtime_setup(
            &application::local_staged_runtime::StagedGaRuntimeRequest {
                workdir: args.workdir.clone(),
                keep_dirs: args.keep_dirs,
                backend: staged_runtime_backend_config_from_staged_ga_args(&args),
            },
        )?;
    let evaluator = application::scott_ga_runtime::StagedScottGaEvaluator::new(
        executor,
        procedure_plan.clone(),
        utf8_workdir.join("ga_stage_runtime"),
    );
    let search_cfg = core.controller_bootstrap.search_cfg.clone();
    let operator_policy = core.lane_metadata.operator_policy.clone();
    let lane_mode = core.lane_metadata.lane_mode;
    let duplicate_policy_mode = core.evidence_policy.duplicate_policy_mode;
    let duplicate_filter_stack = core.evidence_policy.duplicate_filter_stack.clone();
    let workflow_service = application::ga_workflow::GaWorkflowService::new(
        application::workflow_policy::GaWorkflowPolicy::scott_staged_runtime(),
    );
    let hashkey_config = build_ga_artifact_hashkey_config(
        args.use_dreadnaut_keys,
        &args.hashkey_radius,
        args.hashkey_radius_const,
        args.run_dir.join("raw").join("hashkey_identity_runtime"),
    )?;
    let backend_mode_label = match routing_policy.default_backend {
        patina_evaluator::ScottBackendMode::Gulp => "gulp",
        patina_evaluator::ScottBackendMode::JanusMace => "janus_mace",
    };
    let incremental_artifact_sink =
        application::rust_janus_ga_artifacts::StagedScottIncrementalArtifactSink {
            run_dir: &args.run_dir,
            system: &args.system,
            search_cfg: &search_cfg,
            requested_generations: args.ga_generations,
            operator_policy: &operator_policy,
            backend_mode_label,
            hashkey_config: hashkey_config.as_ref(),
            duplicate_trace: &duplicate_trace,
            procedure_traces: evaluator.shared_procedure_traces(),
        };
    let mut emulate_gate_summary = None;
    let mut emulate_gate_decision_traces = Vec::new();
    let mut emulate_gate_batch_traces = Vec::new();
    let execution = if args.emulate_uncertainty_gate {
        let emulate_output_dir = args.run_dir.join("emulate");
        fs::create_dir_all(&emulate_output_dir).with_context(|| {
            format!(
                "failed to create emulate output dir `{}`",
                emulate_output_dir.display()
            )
        })?;
        let projector = build_emulate_feature_projector(
            args.emulate_feature_projector,
            args.emulate_uv_bin.as_deref(),
            args.emulate_project_dir.as_deref(),
            &args.emulate_environment,
        )?;
        let surrogate = patina_emulate::UvSurrogateRuntimeAdapter::new(
            patina_emulate::UvSurrogateRuntimeConfig {
                uv_bin: args
                    .emulate_uv_bin
                    .clone()
                    .unwrap_or_else(|| PathBuf::from("uv")),
                project_dir: absolutize_path(
                    args.emulate_project_dir
                        .as_deref()
                        .unwrap_or_else(|| Path::new("crates/patina-emulate/python")),
                )?,
                work_root: emulate_output_dir,
                environment_name: args.emulate_environment.clone(),
                extras: vec!["autoemulate".into()],
                native_tls: true,
                timeout: Some(Duration::from_secs(args.emulate_timeout_secs)),
                checkpointing: patina_emulate::RuntimeCheckpointConfig::enabled(),
            },
        );
        let gated_evaluator = application::ga_emulate_gate::EmulateGatedGaEvaluator::new(
            &evaluator,
            &*projector,
            &surrogate,
            application::ga_emulate_gate::EmulateGatedGaConfig {
                campaign_id: patina_emulate::CampaignId(
                    args.emulate_campaign_id.clone().unwrap_or_else(|| {
                        format!(
                            "campaign-{}",
                            args.run_dir
                                .file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or("staged-ga")
                        )
                    }),
                ),
                branch_id: patina_emulate::BranchId(args.emulate_branch_id.clone()),
                objective: args.emulate_objective.into(),
                fidelity: args.emulate_fidelity.into(),
                surrogate: patina_emulate::SurrogateConfig {
                    model_variant: args.emulate_model_variant.into(),
                    epochs: args.emulate_epochs,
                    num_inducing: args.emulate_num_inducing,
                    batch_size: args.emulate_batch_size,
                    lr: args.emulate_lr,
                    n_bootstraps: args.emulate_n_bootstraps,
                    random_seed: args.emulate_random_seed,
                    log_level: args.emulate_log_level.clone(),
                    objective: args.emulate_surrogate_objective.into(),
                    primary_target_index: 0,
                    checkpoint: None,
                },
                target_name: args.emulate_target_name.clone(),
                target_unit: Some(args.emulate_target_unit.clone()),
                provenance_label: Some("patina_driver.ga_emulate_uncertainty_gate".into()),
                warmup_generations: args.emulate_warmup_generations,
                min_training_observations: args.emulate_min_training_observations,
                uncertainty_threshold: args.emulate_uncertainty_threshold,
            },
        )?;
        let execution = workflow_service.execute_with_generation_sink(
            application::ga_workflow::GaWorkflowRequest {
                requested_generations: ga_args.ga_generations,
            },
            core,
            &gated_evaluator,
            Some(&incremental_artifact_sink),
        )?;
        emulate_gate_summary = Some(gated_evaluator.run_summary());
        emulate_gate_decision_traces = gated_evaluator.decision_traces();
        emulate_gate_batch_traces = gated_evaluator.batch_traces();
        execution
    } else {
        workflow_service.execute_with_generation_sink(
            application::ga_workflow::GaWorkflowRequest {
                requested_generations: ga_args.ga_generations,
            },
            core,
            &evaluator,
            Some(&incremental_artifact_sink),
        )?
    };
    let procedure_traces = evaluator.procedure_traces();
    let duplicate_traces = application::rust_janus_ga::snapshot_duplicate_trace(&duplicate_trace);
    let artifact_metadata = staged_scott_ga_artifact_metadata_from_args(&args);

    let artifact_sink = application::rust_janus_ga_artifacts::StagedScottGaArtifactSink {
        metadata: &artifact_metadata,
        base_candidate_source_path: core_request.base_candidate_source_path.as_deref(),
        search_cfg: &search_cfg,
        operator_policy: &operator_policy,
        hashkey_config: hashkey_config.as_ref(),
        lane_mode,
        duplicate_policy_mode,
        duplicate_filter_stack: &duplicate_filter_stack,
        duplicate_traces: &duplicate_traces,
        routing_policy: &routing_policy,
        procedure_plan: &procedure_plan,
        procedure_traces: &procedure_traces,
        emulate_gate_summary: emulate_gate_summary.as_ref(),
        emulate_gate_decision_traces: (!emulate_gate_decision_traces.is_empty())
            .then_some(emulate_gate_decision_traces.as_slice()),
        emulate_gate_batch_traces: (!emulate_gate_batch_traces.is_empty())
            .then_some(emulate_gate_batch_traces.as_slice()),
        workflow_policy: workflow_service.policy(),
    };
    let summary = application::ports::GaRunArtifactSink::persist_run(&artifact_sink, &execution)?;

    println!(
        "{}",
        serde_json::to_string_pretty(&summary)
            .context("failed to serialize staged Scott GA summary")?
    );
    Ok(())
}

fn build_bh_scientific_config_from_args(
    args: &RunBasinHoppingArgs,
) -> Result<patina_types::BhScientificConfig> {
    let mut config = application::basin_hopping::default_bh_scientific_config(
        args.bh_steps.max(1),
        args.walkers.max(1),
        args.temperature,
        args.step_size,
    );
    config.method = match args.bh_method {
        DriverBhMethodCli::Relax => patina_types::BhMethodRecord::Relax,
        DriverBhMethodCli::Fixed => patina_types::BhMethodRecord::Fixed,
        DriverBhMethodCli::Oscillate => patina_types::BhMethodRecord::Oscillate {
            high_temperature_steps: args.n_high_temperature.ok_or_else(|| {
                anyhow!("--n-high-temperature is required when --bh-method oscillate")
            })?,
            low_temperature_steps: args.n_low_temperature.ok_or_else(|| {
                anyhow!("--n-low-temperature is required when --bh-method oscillate")
            })?,
        },
    };
    config.acceptance_rule = match args.bh_accept {
        DriverBhAcceptCli::Metropolis => patina_types::BhAcceptanceRuleRecord::Metropolis {
            temperature: args.temperature,
        },
        DriverBhAcceptCli::Quench => patina_types::BhAcceptanceRuleRecord::Quench,
    };
    if let Some(value) = args.dynamic_threshold {
        config.dynamic_threshold = value;
    }
    if let Some(value) = args.moveclass_threshold {
        config.moveclass_threshold = value;
    }
    if let Some(value) = args.max_dynamic_step_multiplier {
        config.max_dynamic_step_multiplier = value;
    }
    if let Some(value) = args.prob_switch_atoms {
        config.prob_switch_atoms = value;
    }
    if let Some(value) = args.prob_switch_cations {
        config.prob_switch_cations = value;
    }
    if let Some(value) = args.prob_mutate_cluster {
        config.prob_mutate_cluster = value;
    }
    if let Some(value) = args.prob_twist_cluster {
        config.prob_twist_cluster = value;
    }
    if let Some(value) = args.prob_translate_cluster {
        config.prob_translate_cluster = value;
    }
    if let Some(value) = args.prob_rotate_cluster {
        config.prob_rotate_cluster = value;
    }
    Ok(config)
}

fn run_basin_hopping(args: RunBasinHoppingArgs) -> Result<()> {
    let scientific_config = build_bh_scientific_config_from_args(&args)?;
    let summary = application::local_sampling_runtime::run_basin_hopping(
        application::local_sampling_runtime::BasinHoppingRunRequest {
            candidate_json: args.candidate_json,
            run_dir: args.run_dir,
            workdir: args.workdir,
            system: args.system,
            scientific_config,
            enforce_container: args.enforce_container,
            boundary: args.boundary,
            seed: args.seed,
            backend: application::local_sampling_runtime::LocalSamplingBackendConfig {
                backend: args.backend.into(),
                timeout: args.timeout_secs.map(Duration::from_secs),
                executable: args.executable,
                master_gin_template: args.master_gin_template,
                run_job_template: args.run_job_template,
                atoms_in_template: args.atoms_in_template,
                jobs_template: args.jobs_template,
                python_bin: args.python_bin,
                janus_adapter_script: args.janus_adapter_script,
                janus_arch: args.janus_arch,
                janus_model: args.janus_model,
                janus_device: args.janus_device,
                janus_dtype: args.janus_dtype,
                janus_mode: args.janus_mode.into(),
                janus_optimizer: args.janus_optimizer.into(),
                janus_fmax: args.janus_fmax,
                janus_steps: args.janus_steps,
            },
        },
    )?;

    println!(
        "{}",
        serde_json::to_string_pretty(&summary)
            .context("failed to serialize basin-hopping summary")?
    );
    Ok(())
}

fn run_energy_lid(args: RunEnergyLidArgs) -> Result<()> {
    let summary = application::local_sampling_runtime::run_energy_lid(
        application::local_sampling_runtime::EnergyLidRunRequest {
            source_run_dir: args.source_run_dir,
            run_dir: args.run_dir,
            workdir: args.workdir,
            system: args.system,
            top_n: args.top_n,
            lid_levels: args.lid_levels,
            lid_increment: args.lid_increment,
            steps_per_lid: args.steps_per_lid,
            quench_steps: args.quench_steps,
            runners_per_lid: args.runners_per_lid,
            step_size: args.step_size,
            enforce_container: args.enforce_container,
            boundary: args.boundary,
            seed: args.seed,
            backend: application::local_sampling_runtime::LocalSamplingBackendConfig {
                backend: args.backend.into(),
                timeout: args.timeout_secs.map(Duration::from_secs),
                executable: args.executable,
                master_gin_template: args.master_gin_template,
                run_job_template: args.run_job_template,
                atoms_in_template: args.atoms_in_template,
                jobs_template: args.jobs_template,
                python_bin: args.python_bin,
                janus_adapter_script: args.janus_adapter_script,
                janus_arch: args.janus_arch,
                janus_model: args.janus_model,
                janus_device: args.janus_device,
                janus_dtype: args.janus_dtype,
                janus_mode: args.janus_mode.into(),
                janus_optimizer: args.janus_optimizer.into(),
                janus_fmax: args.janus_fmax,
                janus_steps: args.janus_steps,
            },
        },
    )?;

    println!(
        "{}",
        serde_json::to_string_pretty(&summary).context("failed to serialize energy-lid summary")?
    );
    Ok(())
}

fn run_simulated_annealing(args: RunSimulatedAnnealingArgs) -> Result<()> {
    let summary = application::local_sampling_runtime::run_simulated_annealing(
        application::local_sampling_runtime::SimulatedAnnealingRunRequest {
            source_run_dir: args.source_run_dir,
            run_dir: args.run_dir,
            workdir: args.workdir,
            system: args.system,
            top_n: args.top_n,
            anneal_steps: args.anneal_steps,
            initial_temperature: args.initial_temperature,
            temperature_scale: args.temperature_scale,
            hold_steps: args.hold_steps,
            quench_steps: args.quench_steps,
            step_size: args.step_size,
            enforce_container: args.enforce_container,
            boundary: args.boundary,
            seed: args.seed,
            backend: application::local_sampling_runtime::LocalSamplingBackendConfig {
                backend: args.backend.into(),
                timeout: args.timeout_secs.map(Duration::from_secs),
                executable: args.executable,
                master_gin_template: args.master_gin_template,
                run_job_template: args.run_job_template,
                atoms_in_template: args.atoms_in_template,
                jobs_template: args.jobs_template,
                python_bin: args.python_bin,
                janus_adapter_script: args.janus_adapter_script,
                janus_arch: args.janus_arch,
                janus_model: args.janus_model,
                janus_device: args.janus_device,
                janus_dtype: args.janus_dtype,
                janus_mode: args.janus_mode.into(),
                janus_optimizer: args.janus_optimizer.into(),
                janus_fmax: args.janus_fmax,
                janus_steps: args.janus_steps,
            },
        },
    )?;

    println!(
        "{}",
        serde_json::to_string_pretty(&summary)
            .context("failed to serialize simulated annealing summary")?
    );
    Ok(())
}

fn run_scan_surface(args: RunScanSurfaceArgs) -> Result<()> {
    let summary = application::local_sampling_runtime::run_scan_surface(
        application::local_sampling_runtime::ScanSurfaceRunRequest {
            initial_candidate_json: args.initial_candidate_json,
            restart_candidate_json: args.restart_candidate_json,
            run_dir: args.run_dir,
            workdir: args.workdir,
            system: args.system,
            steps_per_scan: args.steps_per_scan,
            temperature: args.temperature,
            base_step_size: args.base_step_size,
            dynamic_threshold: args.dynamic_threshold,
            move_mode: args.move_mode.into(),
            acceptance_mode: args.acceptance_mode.into(),
            evaluate_initial_state: args.evaluate_initial_state,
            center: [args.center_x, args.center_y, args.center_z],
            boundary: [args.boundary_x, args.boundary_y, args.boundary_z],
            above_surface: args.above_surface,
            seed: args.seed,
            backend: application::local_sampling_runtime::LocalSamplingBackendConfig {
                backend: args.backend.into(),
                timeout: args.timeout_secs.map(Duration::from_secs),
                executable: args.executable,
                master_gin_template: args.master_gin_template,
                run_job_template: args.run_job_template,
                atoms_in_template: args.atoms_in_template,
                jobs_template: args.jobs_template,
                python_bin: args.python_bin,
                janus_adapter_script: args.janus_adapter_script,
                janus_arch: args.janus_arch,
                janus_model: args.janus_model,
                janus_device: args.janus_device,
                janus_dtype: args.janus_dtype,
                janus_mode: args.janus_mode.into(),
                janus_optimizer: args.janus_optimizer.into(),
                janus_fmax: args.janus_fmax,
                janus_steps: args.janus_steps,
            },
        },
    )?;

    println!(
        "{}",
        serde_json::to_string_pretty(&summary)
            .context("failed to serialize scan-surface summary")?
    );
    Ok(())
}

fn run_solid_solutions(args: RunSolidSolutionsArgs) -> Result<()> {
    let rdf_config = if args.compute_rdf {
        Some(
            application::solid_solutions::SolidSolutionsRdfConfig::with_values(
                args.rdf_cutoff,
                args.rdf_step,
                args.rdf_steps,
                args.rdf_sigma,
                args.rdf_zero,
                args.rdf_accuracy,
                true,
            )?,
        )
    } else {
        None
    };
    let summary = application::local_sampling_runtime::run_solid_solutions(
        application::local_sampling_runtime::SolidSolutionsRunRequest {
            initial_candidate_json: args.initial_candidate_json,
            proposal_candidate_json: args.proposal_candidate_json,
            run_dir: args.run_dir,
            workdir: args.workdir,
            system: args.system,
            steps: args.steps,
            temperature: args.temperature,
            move_mode: args.move_mode.into(),
            acceptance_mode: args.acceptance_mode.into(),
            max_exchanges: args.max_exchanges,
            skip_evaluation: args.skip_evaluation,
            imported_hashkeys: args.imported_hashkey,
            imported_hashkey_files: args.imported_hashkeys_file,
            atoms_in_template: args.atoms_in_template.clone(),
            use_dreadnaut_keys: args.use_dreadnaut_keys,
            hashkey_radius: args.hashkey_radius,
            hashkey_radius_const: args.hashkey_radius_const,
            geometry_min_distance: args.geometry_min_distance,
            statistics_enabled: args.ss_stats,
            statistics_backup_interval: args.ss_stats_backup,
            rdf_config,
            seed: args.seed,
            backend: application::local_sampling_runtime::LocalSamplingBackendConfig {
                backend: args.backend.into(),
                timeout: args.timeout_secs.map(Duration::from_secs),
                executable: args.executable,
                master_gin_template: args.master_gin_template,
                run_job_template: args.run_job_template,
                atoms_in_template: args.atoms_in_template,
                jobs_template: args.jobs_template,
                python_bin: args.python_bin,
                janus_adapter_script: args.janus_adapter_script,
                janus_arch: args.janus_arch,
                janus_model: args.janus_model,
                janus_device: args.janus_device,
                janus_dtype: args.janus_dtype,
                janus_mode: args.janus_mode.into(),
                janus_optimizer: args.janus_optimizer.into(),
                janus_fmax: args.janus_fmax,
                janus_steps: args.janus_steps,
            },
        },
    )?;

    println!(
        "{}",
        serde_json::to_string_pretty(&summary)
            .context("failed to serialize solid-solutions summary")?
    );
    Ok(())
}

fn perturb_cluster(args: PerturbClusterArgs) -> Result<()> {
    let execution = application::local_perturbation_runtime::run_cluster_perturbation(
        application::local_perturbation_runtime::ClusterPerturbationRunRequest {
            candidate_json: args.candidate_json,
            run_dir: args.run_dir,
            system: args.system,
            count: args.count,
            sigma: args.sigma,
            max_displacement: args.max_displacement,
            validate_min_distance: args.validate_min_distance,
            duplicate_threshold: args.duplicate_threshold,
            include_p_orbitals: args.include_p_orbitals,
            duplicate_screening_mode: match args.duplicate_screening_mode {
                PerturbClusterDuplicateScreeningModeCli::GlobalOverlap => {
                    patina_perturber::DuplicateScreeningMode::GlobalOverlap
                }
                PerturbClusterDuplicateScreeningModeCli::EnvironmentAssignment => {
                    patina_perturber::DuplicateScreeningMode::EnvironmentAssignment
                }
            },
            environment_width_cutoff: args.environment_width_cutoff,
            environment_max_atoms_in_sphere: args.environment_max_atoms_in_sphere,
            environment_s_orbital_count: args.environment_s_orbital_count,
            environment_p_orbital_count: args.environment_p_orbital_count,
            seed: args.seed,
        },
    )?;

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "source_label": execution.source_cluster.label,
            "count": execution.count,
            "source_fingerprint_dimension": execution.source_fingerprint.values.len(),
            "duplicate_threshold": execution.duplicate_threshold,
            "duplicate_screening_mode": match args.duplicate_screening_mode {
                PerturbClusterDuplicateScreeningModeCli::GlobalOverlap => "global_overlap",
                PerturbClusterDuplicateScreeningModeCli::EnvironmentAssignment => "environment_assignment",
            },
            "duplicate_vs_source_count": execution.variant_analyses.iter().filter(|analysis| analysis.duplicate_vs_source.duplicate).count(),
            "variant_labels": execution.batch.variants.iter().map(|variant| variant.label.clone()).collect::<Vec<_>>()
        }))
        .context("failed to serialize cluster perturbation summary")?
    );
    Ok(())
}

fn generate_surface(args: GenerateSurfaceArgs) -> Result<()> {
    let execution = application::local_framework_surface_runtime::generate_surface(
        application::local_framework_surface_runtime::SurfaceGenerationRunRequest {
            input: application::local_framework_surface_runtime::PeriodicStructureInput {
                candidate_json: args.candidate_json,
                structure_path: args.structure_path,
            },
            run_dir: args.run_dir,
            system: args.system,
            config: SurfaceGenerationConfig {
                miller: MillerIndex::new(args.h, args.k, args.l)?,
                thickness_angstrom: args.thickness,
                vacuum_angstrom: args.vacuum,
                supercell: SurfaceSupercellConfig {
                    repeat_a: args.supercell_a,
                    repeat_b: args.supercell_b,
                },
                cut_strategy: args.cut_strategy.into(),
                cut_offset_fraction: args.cut_offset_fraction,
                slab_reduction: SlabReductionConfig {
                    dedup_slab: args.dedup_slab,
                    reduce_slab_inplane: args.reduce_slab_inplane,
                    dedup: DedupConfig {
                        frac_tol: args.dedup_frac_tol,
                        inplane_only: !args.dedup_wrap_z,
                        require_same_element: !args.dedup_ignore_element,
                    },
                },
                reconstruction: patina_surface::SurfaceReconstructionMode::None,
                termination_bias: SurfaceTerminationBias::Neutral,
            },
        },
    )?;

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "slab_label": execution.result.slab.label,
            "atom_count": execution.result.slab.atoms.len(),
            "periodic_axes": execution.result.slab.periodic_axes,
            "topology_safe_cut": execution.result.diagnostics.topology_safe_cut,
            "chosen_cut_offset_angstrom": execution.result.diagnostics.chosen_cut_offset_angstrom,
            "interplanar_spacing_angstrom": execution.result.diagnostics.interplanar_spacing_angstrom,
            "layer_count": execution.result.diagnostics.layer_count,
            "warnings": execution.result.diagnostics.warnings,
        }))
        .context("failed to serialize surface generation summary")?
    );
    Ok(())
}

fn run_surface_reconstruction(args: RunSurfaceReconstructionArgs) -> Result<()> {
    let execution = application::local_framework_surface_runtime::run_surface_reconstruction(
        application::local_framework_surface_runtime::SurfaceReconstructionRunRequest {
            input: application::local_framework_surface_runtime::PeriodicStructureInput {
                candidate_json: args.candidate_json,
                structure_path: args.structure_path,
            },
            run_dir: args.run_dir,
            workdir: args.workdir,
            system: args.system,
            generation_config: SurfaceGenerationConfig {
                miller: MillerIndex::new(args.h, args.k, args.l)?,
                thickness_angstrom: args.thickness,
                vacuum_angstrom: args.vacuum,
                supercell: SurfaceSupercellConfig {
                    repeat_a: args.supercell_a,
                    repeat_b: args.supercell_b,
                },
                cut_strategy: args.cut_strategy.into(),
                cut_offset_fraction: args.cut_offset_fraction,
                slab_reduction: SlabReductionConfig {
                    dedup_slab: args.dedup_slab,
                    reduce_slab_inplane: args.reduce_slab_inplane,
                    dedup: DedupConfig {
                        frac_tol: args.dedup_frac_tol,
                        inplane_only: !args.dedup_wrap_z,
                        require_same_element: !args.dedup_ignore_element,
                    },
                },
                reconstruction: args.reconstruction.into(),
                termination_bias: SurfaceTerminationBias::Neutral,
            },
            target_face: args.target_face.into(),
            movable_species: args.movable_species,
            site_filter: args.site_filter.into(),
            region_policy: args.region_policy.into(),
            movable_layer_count: args.movable_layer_count,
            layer_z_tolerance_angstrom: args.layer_z_tolerance_angstrom,
            move_family: args.move_family.into(),
            steps: args.steps,
            temperature: args.temperature,
            lateral_fractional_step: args.lateral_fractional_step,
            outward_normal_step_angstrom: args.outward_normal_step_angstrom,
            inward_normal_step_angstrom: args.inward_normal_step_angstrom,
            evaluate_initial_state: args.evaluate_initial_state,
            seed: args.seed,
            top_n_exports: args.top_n_exports,
            backend: application::local_sampling_runtime::LocalSamplingBackendConfig {
                backend: args.backend.into(),
                timeout: args.timeout_secs.map(Duration::from_secs),
                executable: args.executable,
                master_gin_template: args.master_gin_template,
                run_job_template: args.run_job_template,
                atoms_in_template: args.atoms_in_template,
                jobs_template: args.jobs_template,
                python_bin: args.python_bin,
                janus_adapter_script: args.janus_adapter_script,
                janus_arch: args.janus_arch,
                janus_model: args.janus_model,
                janus_device: args.janus_device,
                janus_dtype: args.janus_dtype,
                janus_mode: args.janus_mode.into(),
                janus_optimizer: args.janus_optimizer.into(),
                janus_fmax: args.janus_fmax,
                janus_steps: args.janus_steps,
            },
        },
    )?;

    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "target_face": execution.summary.target_face,
            "movable_indices": execution.summary.movable_indices,
            "steps": execution.summary.steps,
            "accepted_steps": execution.summary.accepted_steps,
            "rejected_acceptance_steps": execution.summary.rejected_acceptance_steps,
            "rejected_evaluation_steps": execution.summary.rejected_evaluation_steps,
            "best_energy": execution.summary.best_energy,
            "final_candidate_label": execution.final_candidate.label,
        }))
        .context("failed to serialize surface reconstruction summary")?
    );
    Ok(())
}

fn analyze_surface_polarity(args: AnalyzeSurfacePolarityArgs) -> Result<()> {
    let execution = application::local_framework_surface_runtime::analyze_surface_polarity(
        application::local_framework_surface_runtime::SurfacePolarityRunRequest {
            input: application::local_framework_surface_runtime::PeriodicStructureInput {
                candidate_json: args.candidate_json,
                structure_path: args.structure_path,
            },
            run_dir: args.run_dir,
            system: args.system,
        },
    )?;

    println!(
        "{}",
        serde_json::to_string_pretty(&execution.report)
            .context("failed to serialize surface polarity report")?
    );
    Ok(())
}

fn analyze_framework_symmetry(args: AnalyzeFrameworkSymmetryArgs) -> Result<()> {
    let execution = application::local_framework_surface_runtime::analyze_framework_symmetry(
        application::local_framework_surface_runtime::FrameworkSymmetryRunRequest {
            input: application::local_framework_surface_runtime::PeriodicStructureInput {
                candidate_json: args.candidate_json,
                structure_path: args.structure_path,
            },
            run_dir: args.run_dir,
            system: args.system,
            tolerance: SymmetryTolerance {
                position_tolerance: args.position_tolerance,
                cell_tolerance: args.cell_tolerance,
            },
        },
    )?;

    println!(
        "{}",
        serde_json::to_string_pretty(&execution.analysis)
            .context("failed to serialize framework symmetry analysis")?
    );
    Ok(())
}

fn normalize_framework(args: NormalizeFrameworkArgs) -> Result<()> {
    let execution = application::local_framework_surface_runtime::normalize_framework(
        application::local_framework_surface_runtime::FrameworkNormalizationRunRequest {
            input: application::local_framework_surface_runtime::PeriodicStructureInput {
                candidate_json: args.candidate_json,
                structure_path: args.structure_path,
            },
            run_dir: args.run_dir,
            system: args.system,
            tolerance: SymmetryTolerance {
                position_tolerance: args.position_tolerance,
                cell_tolerance: args.cell_tolerance,
            },
            target: args.target.into(),
        },
    )?;

    println!(
        "{}",
        serde_json::to_string_pretty(&execution.normalized_framework)
            .context("failed to serialize normalized framework")?
    );
    Ok(())
}

fn run_framework_gcmc(args: RunFrameworkGcmcArgs) -> Result<()> {
    let execution = application::local_framework_surface_runtime::run_framework_gcmc(
        application::local_framework_surface_runtime::FrameworkGcmcRunRequest {
            input: application::local_framework_surface_runtime::PeriodicStructureInput {
                candidate_json: args.candidate_json,
                structure_path: args.structure_path,
            },
            run_dir: args.run_dir,
            workdir: args.workdir,
            system: args.system,
            guest: args.guest,
            temperature_kelvin: args.temperature_kelvin,
            pressure_bar: args.pressure_bar,
            initialization_cycles: args.initialization_cycles,
            production_cycles: args.production_cycles,
            max_guest_count: args.max_guest_count,
            minimum_guest_host_distance: args.minimum_guest_host_distance,
            minimum_guest_guest_distance: args.minimum_guest_guest_distance,
            density_dimensions: [args.density_nx, args.density_ny, args.density_nz],
            density_binning: args.density_binning.into(),
            density_normalization: args.density_normalization.into(),
            energy_histogram_bins: args.energy_histogram_bins,
            energy_histogram_range: (args.energy_histogram_min_ev, args.energy_histogram_max_ev),
            number_histogram_limits: (args.number_histogram_lower, args.number_histogram_upper),
            seed: args.seed,
            python_bin: args.python_bin,
            janus_adapter_script: args.janus_adapter_script,
            janus_arch: args.janus_arch,
            janus_model: args.janus_model,
            janus_device: args.janus_device,
            janus_dtype: args.janus_dtype,
            janus_mode: args.janus_mode.into(),
            janus_fmax: args.janus_fmax,
            janus_steps: args.janus_steps,
            janus_optimizer: args.janus_optimizer.into(),
        },
    )?;

    println!(
        "{}",
        serde_json::to_string_pretty(&execution.result.summary)
            .context("failed to serialize framework GCMC summary")?
    );
    Ok(())
}

#[derive(Debug, Serialize)]
struct StructureSpaceGroupReport {
    stage_dir: PathBuf,
    source_path: PathBuf,
    source_format: application::workflow_input::CandidateInputFormat,
    label: String,
    atom_count: usize,
    tolerance: SymmetryTolerance,
    hall_number: Option<usize>,
    international_number: Option<usize>,
    hm_symbol: Option<String>,
    operation_count: usize,
    orbit_count: usize,
    wyckoff_letter_counts: BTreeMap<String, usize>,
}

fn run_structure_command(args: StructureArgs) -> Result<()> {
    match args.command {
        StructureCommand::SpaceGroup(args) => run_structure_space_group(args),
    }
}

fn construct_stk_zero_d(args: ConstructStkZeroDArgs) -> Result<()> {
    let summary = application::stk_construction::construct_zero_d_topology(
        &application::stk_construction::StkConstructionRequest {
            topology: args.topology.into(),
            output_dir: args.output_dir,
        },
    )?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else {
        println!(
            "constructed STK topology `{}`: atoms={} placements={} bonds={}",
            summary.topology, summary.atom_count, summary.placement_count, summary.bond_count
        );
        println!("  xyz: {}", summary.xyz_path.display());
        println!("  summary: {}", summary.summary_path.display());
        println!("  result: {}", summary.result_path.display());
    }

    Ok(())
}

fn run_structure_space_group(args: StructureSpaceGroupArgs) -> Result<()> {
    let mut config =
        application::workflow_input_config::load_workflow_configuration(args.config.as_deref())?;
    apply_structure_space_group_cli_overrides(&mut config, &args)?;

    let resolved_input = application::workflow_input::resolve_single_candidate_input(
        application::workflow_input::SingleCandidateInputContract::space_group(),
        config.stage.dir.as_deref(),
        &application::workflow_input::CandidateInputSelection::from_structure_config(&config.input),
    )?;
    let framework =
        PeriodicFramework::try_from_candidate(&resolved_input.candidate).with_context(|| {
            format!(
                "space-group analysis requires a fully periodic structure; source `{}`",
                resolved_input.source_path.display()
            )
        })?;
    let tolerance = SymmetryTolerance {
        position_tolerance: config.structure.space_group.position_tolerance,
        cell_tolerance: config.structure.space_group.cell_tolerance,
    };
    let analysis = MoyoSymmetryAnalyzer
        .analyze(&framework, tolerance)
        .with_context(|| {
            format!(
                "failed to analyze space group for `{}`",
                resolved_input.source_path.display()
            )
        })?;

    let report = StructureSpaceGroupReport {
        stage_dir: resolved_input.stage_dir,
        source_path: resolved_input.source_path,
        source_format: resolved_input.source_format,
        label: resolved_input.candidate.label,
        atom_count: framework.atom_count(),
        tolerance,
        hall_number: analysis.hall_number,
        international_number: analysis.international_number,
        hm_symbol: analysis.hm_symbol,
        operation_count: analysis.operations.len(),
        orbit_count: analysis.orbits.len(),
        wyckoff_letter_counts: count_wyckoff_letters(&analysis.wyckoff_letters),
    };

    if config.structure.space_group.output_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .context("failed to serialize structure space-group report")?
        );
    } else {
        println!("{}", render_structure_space_group_report(&report));
    }

    Ok(())
}

fn apply_structure_space_group_cli_overrides(
    config: &mut application::workflow_input_config::WorkflowConfiguration,
    args: &StructureSpaceGroupArgs,
) -> Result<()> {
    if let Some(stage_dir) = &args.stage_dir {
        config.stage.dir = Some(stage_dir.clone());
    }
    if let Some(position_tolerance) = args.position_tolerance {
        config.structure.space_group.position_tolerance = position_tolerance;
    }
    if let Some(cell_tolerance) = args.cell_tolerance {
        config.structure.space_group.cell_tolerance = cell_tolerance;
    }
    if args.json {
        config.structure.space_group.output_json = true;
    }

    let positional_stage_dir = args.path.as_deref().is_some_and(Path::is_dir);
    if positional_stage_dir && args.stage_dir.is_some() {
        bail!("provide either positional stage directory or `--stage-dir`, not both");
    }
    let input_override_count = usize::from(args.candidate_json.is_some())
        + usize::from(args.structure_path.is_some())
        + usize::from(args.path.is_some() && !positional_stage_dir);
    if input_override_count > 1 {
        bail!(
            "provide only one of positional input path, `--candidate-json`, or `--structure-path`"
        );
    }

    if let Some(path) = &args.path {
        if positional_stage_dir {
            config.stage.dir = Some(path.clone());
            config.input.candidate_json = None;
            config.input.structure_path = None;
        } else {
            let mut selection =
                application::workflow_input::CandidateInputSelection::from_structure_config(
                    &config.input,
                );
            selection.set_path_by_format(path);
            config.input.candidate_json = selection.candidate_json;
            config.input.structure_path = selection.structure_path;
        }
    }
    if let Some(path) = &args.candidate_json {
        config.input.candidate_json = Some(path.clone());
        config.input.structure_path = None;
    }
    if let Some(path) = &args.structure_path {
        config.input.candidate_json = None;
        config.input.structure_path = Some(path.clone());
    }

    Ok(())
}

fn render_optional_usize(value: Option<usize>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn render_optional_string(value: Option<&str>) -> String {
    value
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "unknown".to_string())
}

fn count_wyckoff_letters(values: &[String]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for value in values {
        *counts.entry(value.clone()).or_insert(0) += 1;
    }
    counts
}

fn render_wyckoff_letter_counts(values: &BTreeMap<String, usize>) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values
            .iter()
            .map(|(letter, count)| format!("{letter}:{count}"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn render_structure_space_group_report(report: &StructureSpaceGroupReport) -> String {
    [
        format!("source: {}", report.source_path.display()),
        format!("stage: {}", report.stage_dir.display()),
        format!("label: {}", report.label),
        format!("atoms: {}", report.atom_count),
        format!(
            "space_group: {} {}",
            render_optional_usize(report.international_number),
            render_optional_string(report.hm_symbol.as_deref())
        ),
        format!("hall_number: {}", render_optional_usize(report.hall_number)),
        format!("symmetry_operations: {}", report.operation_count),
        format!("orbits: {}", report.orbit_count),
        format!(
            "wyckoff_letter_counts: {}",
            render_wyckoff_letter_counts(&report.wyckoff_letter_counts)
        ),
        format!(
            "tolerance: position={} cell={}",
            report.tolerance.position_tolerance, report.tolerance.cell_tolerance
        ),
    ]
    .join("\n")
}

fn enforce_external_initial_uniqueness<D: DuplicatePolicy>(
    controller: &mut ScottParityGeneticAlgorithm<D>,
    candidates: &mut [Candidate],
    atom_specs: Option<&[AtomSpecRecord]>,
    radius_mode: &str,
    radius_const: f64,
    hkg_path: &Path,
    scratch_dir: &Path,
) -> Result<()> {
    fs::create_dir_all(scratch_dir).with_context(|| {
        format!(
            "failed to create initial hashkey scratch dir `{}`",
            scratch_dir.display()
        )
    })?;

    let mut seen = BTreeSet::new();
    for (idx, candidate) in candidates.iter_mut().enumerate() {
        let mut accepted = false;
        for attempt in 0..64usize {
            let hash = application::driver_support::build_external_hashkey(
                candidate,
                atom_specs,
                radius_mode,
                radius_const,
                hkg_path,
                scratch_dir,
                &format!("init_{idx:04}_{attempt:02}"),
            )?;
            match hash {
                Some(hash) if seen.contains(&hash) => {
                    *candidate = controller.generate_initial_candidate(idx + attempt + 1)?;
                }
                Some(hash) => {
                    seen.insert(hash);
                    accepted = true;
                    break;
                }
                None => {
                    accepted = true;
                    break;
                }
            }
        }

        if !accepted {
            let final_hash = application::driver_support::build_external_hashkey(
                candidate,
                atom_specs,
                radius_mode,
                radius_const,
                hkg_path,
                scratch_dir,
                &format!("init_{idx:04}_final"),
            )?;
            if let Some(hash) = final_hash {
                seen.insert(hash);
            }
        }
    }

    Ok(())
}

fn read_candidate_json(path: &Path) -> Result<Candidate> {
    let candidate_raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read candidate JSON from `{}`", path.display()))?;
    let candidate: Candidate = serde_json::from_str(&candidate_raw)
        .with_context(|| format!("failed to parse candidate JSON from `{}`", path.display()))?;
    candidate.validate().map_err(|err| {
        anyhow!(
            "candidate from `{}` failed validation: {err:?}",
            path.display()
        )
    })?;
    let _support_status =
        application::driver_support::candidate_dimensionality_support_label(&candidate);
    Ok(candidate)
}

fn default_worker_count() -> usize {
    std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1)
        .max(1)
}

fn required_arg<T: Clone>(value: Option<T>, flag: &str, backend: &str) -> Result<T> {
    value.ok_or_else(|| anyhow!("`--{flag}` is required when backend is `{backend}`"))
}

fn absolutize_path(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    let cwd = std::env::current_dir().context("failed to resolve current working directory")?;
    Ok(cwd.join(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn atom_specs() -> Vec<AtomSpecRecord> {
        vec![
            AtomSpecRecord {
                species: "O".to_string(),
                covalent_radius: 0.73,
                ionic_radius: 1.26,
            },
            AtomSpecRecord {
                species: "Mg".to_string(),
                covalent_radius: 1.10,
                ionic_radius: 0.86,
            },
        ]
    }

    fn base_atoms() -> Vec<StructureAtom> {
        vec![
            StructureAtom {
                species: "Mg".to_string(),
                coords: [0.0, 0.0, 0.0],
            },
            StructureAtom {
                species: "Mg".to_string(),
                coords: [2.0, 0.0, 0.0],
            },
            StructureAtom {
                species: "O".to_string(),
                coords: [0.0, 2.0, 0.0],
            },
            StructureAtom {
                species: "O".to_string(),
                coords: [2.0, 2.0, 0.0],
            },
        ]
    }

    #[test]
    fn compute_structure_hashkey_radius_matches_mgo_ir_default() {
        let species_counts = BTreeMap::from([("Mg".to_string(), 2_usize), ("O".to_string(), 2)]);
        let radius = application::scott_topology_export::compute_structure_hashkey_radius(
            &species_counts,
            &atom_specs(),
            "IR",
            0.4,
        )
        .unwrap();
        assert!((radius - 2.52).abs() < 1e-9);
    }

    #[test]
    fn external_hashkey_radius_offset_matches_native_solid_solution_quirk() {
        let species_counts = BTreeMap::from([("Mg".to_string(), 2_usize), ("O".to_string(), 2)]);
        let base_radius = application::scott_topology_export::compute_structure_hashkey_radius(
            &species_counts,
            &atom_specs(),
            "IR",
            0.4,
        )
        .unwrap();
        let solid_solution_radius = base_radius + 0.4;
        assert!((solid_solution_radius - 2.92).abs() < 1e-9);
    }

    #[test]
    fn graph_summaries_are_invariant_to_translation_and_permutation() {
        let atoms = base_atoms();
        let translated = atoms
            .iter()
            .map(|atom| StructureAtom {
                species: atom.species.clone(),
                coords: [
                    atom.coords[0] + 5.0,
                    atom.coords[1] - 3.0,
                    atom.coords[2] + 1.5,
                ],
            })
            .collect::<Vec<_>>();
        let permuted = vec![
            translated[3].clone(),
            translated[1].clone(),
            translated[0].clone(),
            translated[2].clone(),
        ];

        let radius = 2.52;
        let edges_a = application::scott_topology_export::build_structure_edges_with_lattice(
            &atoms,
            None,
            [false, false, false],
            radius,
        );
        let edges_b = application::scott_topology_export::build_structure_edges_with_lattice(
            &permuted,
            None,
            [false, false, false],
            radius,
        );
        assert_eq!(
            application::scott_topology_export::summarize_edge_pairs(&atoms, &edges_a),
            application::scott_topology_export::summarize_edge_pairs(&permuted, &edges_b)
        );
        assert_eq!(
            application::scott_topology_export::summarize_coordination_histograms(&atoms, &edges_a),
            application::scott_topology_export::summarize_coordination_histograms(
                &permuted, &edges_b,
            )
        );
    }

    #[test]
    fn classify_topology_verdict_prefers_exact_hashkey_identity() {
        assert_eq!(
            application::scott_topology_export::classify_topology_verdict(Some(true), 8, 8),
            "identical"
        );
        assert_eq!(
            application::scott_topology_export::classify_topology_verdict(Some(false), 2, 2),
            "near"
        );
        assert_eq!(
            application::scott_topology_export::classify_topology_verdict(Some(false), 8, 2),
            "different"
        );
    }

    #[test]
    fn dimensionality_support_labels_match_current_workflow_boundary() {
        let cluster = Candidate {
            species: vec!["Mg".into()],
            fractional_coords: vec![[0.0, 0.0, 0.0]],
            lattice: None,
            periodic_axes: [false, false, false],
            label: "cluster".into(),
        };
        let slab = Candidate {
            species: vec!["Mg".into()],
            fractional_coords: vec![[0.0, 0.0, 0.0]],
            lattice: Some([[4.0, 0.0, 0.0], [0.0, 4.0, 0.0], [0.0, 0.0, 20.0]]),
            periodic_axes: [true, true, false],
            label: "slab".into(),
        };
        let bulk = Candidate {
            species: vec!["Mg".into()],
            fractional_coords: vec![[0.0, 0.0, 0.0]],
            lattice: Some([[4.0, 0.0, 0.0], [0.0, 4.0, 0.0], [0.0, 0.0, 4.0]]),
            periodic_axes: [true, true, true],
            label: "bulk".into(),
        };

        assert_eq!(
            application::driver_support::candidate_dimensionality_label(&cluster),
            "0D"
        );
        assert_eq!(
            application::driver_support::candidate_dimensionality_support_label(&cluster),
            "native-parity-search-eligible"
        );
        assert_eq!(
            application::driver_support::candidate_dimensionality_label(&slab),
            "2D"
        );
        assert_eq!(
            application::driver_support::candidate_dimensionality_support_label(&slab),
            "topology-and-data-only"
        );
        assert_eq!(
            application::driver_support::candidate_dimensionality_label(&bulk),
            "3D"
        );
        assert_eq!(
            application::driver_support::candidate_dimensionality_support_label(&bulk),
            "native-parity-search-eligible"
        );
    }

    #[test]
    fn native_scott_search_guard_rejects_partial_periodic_candidates() {
        let slab = Candidate {
            species: vec!["Mg".into()],
            fractional_coords: vec![[0.0, 0.0, 0.0]],
            lattice: Some([[4.0, 0.0, 0.0], [0.0, 4.0, 0.0], [0.0, 0.0, 20.0]]),
            periodic_axes: [true, true, false],
            label: "slab".into(),
        };

        let err = application::driver_support::ensure_native_scott_search_candidate(
            &slab,
            "staged Scott runtime",
        )
        .expect_err("partial periodic candidate should be rejected");
        let message = err.to_string();
        assert!(message.contains("staged Scott runtime"));
        assert!(message.contains("2D"));
        assert!(message.contains("topology-and-data-only"));
    }

    fn empty_legacy_scott_args() -> RunScottSearchArgs {
        RunScottSearchArgs {
            config: None,
            job_type: None,
            mode: None,
            scott_bin: None,
            data_dir: None,
            workdir: None,
            run_dir: None,
            system: None,
            bh_steps: None,
            ga_generations: None,
            population: None,
            seed: None,
            evaluator_backend: None,
            janus_python: None,
            janus_adapter: None,
            janus_mode: None,
            janus_arch: None,
            janus_model: None,
            janus_device: None,
            janus_dtype: None,
            janus_fmax: None,
            janus_steps: None,
            temperature: None,
            step_size: None,
            boundary: None,
            collapse: None,
            fragment: None,
            dspecies: None,
            output_level: None,
            use_top_analysis: None,
            use_dreadnaut_keys: None,
            hashkey_radius: None,
            hashkey_radius_const: None,
            timeout_secs: None,
        }
    }

    #[test]
    fn legacy_scott_job_type_resolution_accepts_mode_shorthand() {
        let mut args = empty_legacy_scott_args();
        args.mode = Some(SearchMode::Ga);
        let mut overrides = Vec::new();
        let job_type = resolve_legacy_scott_job_type(&args, None, &mut overrides).expect("mode");
        assert_eq!(
            job_type,
            application::legacy_scott::LegacyScottJobType::GeneticAlgorithm
        );
    }

    #[test]
    fn legacy_scott_job_type_resolution_rejects_conflicting_inputs() {
        let mut args = empty_legacy_scott_args();
        args.mode = Some(SearchMode::Bh);
        args.job_type = Some(LegacyScottJobTypeCli::ProductionRun);
        let mut overrides = Vec::new();
        let error =
            resolve_legacy_scott_job_type(&args, None, &mut overrides).expect_err("conflict");
        assert!(error.to_string().contains("incompatible"));
    }

    #[test]
    fn recoverable_outputs_accept_generic_native_runtime_roots() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("KLMC.out"), "native start").expect("klmc.out");
        fs::create_dir(dir.path().join("run")).expect("run dir");
        fs::create_dir(dir.path().join("top_structures")).expect("top_structures dir");

        assert!(scott_search_has_recoverable_outputs(dir.path()));
    }

    fn write_staged_scott_input_fixture(dir: &Path) -> (PathBuf, PathBuf, PathBuf) {
        let master = dir.join("Master.gin");
        let run_job = dir.join("run.job");
        let atoms_in = dir.join("atoms.in");
        fs::write(
            &master,
            r#"
opti
cartesian
Mg core 0.0 0.0 0.0
O core 1.0 1.0 1.0
species
Mg core 2.0
O core -2.0
"#,
        )
        .unwrap();
        fs::write(&run_job, "ngen=1\n").unwrap();
        fs::write(&atoms_in, "Mg 0.0 0.0 0.0\n").unwrap();
        (master, run_job, atoms_in)
    }

    #[test]
    fn resolve_staged_scott_inputs_uses_bundle_dir_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let (_master, run_job, atoms_in) = write_staged_scott_input_fixture(dir.path());
        let resolved = application::local_staged_runtime::resolve_staged_scott_inputs(
            Some(dir.path()),
            None,
            Some(run_job.clone()),
            Some(atoms_in.clone()),
            "test staged runtime",
        )
        .expect("bundled Scott inputs should resolve");

        assert!(resolved.master_gin_template.ends_with("Master.gin"));
        assert_eq!(resolved.run_job_template, run_job);
        assert!(resolved.atoms_in_template.is_some());
        assert_eq!(
            resolved.atoms_in_template.as_ref().expect("atoms.in"),
            &atoms_in
        );
    }

    #[test]
    fn resolve_staged_scott_inputs_prefers_explicit_paths() {
        let dir = tempfile::tempdir().unwrap();
        let (master, run_job, _atoms_in) = write_staged_scott_input_fixture(dir.path());
        let resolved = application::local_staged_runtime::resolve_staged_scott_inputs(
            Some(dir.path()),
            Some(master.clone()),
            Some(run_job.clone()),
            None,
            "test staged runtime",
        )
        .expect("explicit paths should override bundled defaults");

        assert_eq!(resolved.master_gin_template, master);
        assert_eq!(resolved.run_job_template, run_job);
    }

    #[test]
    fn dreadnaut_graph_text_sorts_color_partitions_by_population_count() {
        let candidate = Candidate {
            species: vec!["Mg".into(), "O".into(), "Mg".into()],
            fractional_coords: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            lattice: None,
            periodic_axes: [false, false, false],
            label: "mgo".into(),
        };

        let graph = build_dreadnaut_graph_text(&candidate, 3.0, &atom_specs());
        let f_line = graph
            .lines()
            .find(|line| line.starts_with("f=["))
            .expect("f-line present");

        assert_eq!(f_line, "f=[1,|0,2,]");
    }

    #[test]
    fn dreadnaut_graph_text_filters_placeholder_species_and_keeps_terminal_z() {
        let candidate = Candidate {
            species: vec!["Mg".into(), "X".into(), "O".into()],
            fractional_coords: vec![[0.0, 0.0, 0.0], [9.0, 9.0, 9.0], [1.0, 0.0, 0.0]],
            lattice: None,
            periodic_axes: [false, false, false],
            label: "filtered".into(),
        };

        let graph = build_dreadnaut_graph_text(&candidate, 3.0, &atom_specs());
        let lines = graph.lines().collect::<Vec<_>>();

        assert!(lines.contains(&"n=2 g"));
        assert_eq!(lines.last().copied(), Some("z"));
    }

    #[test]
    fn run_rust_janus_search_cli_leaves_atoms_override_unset_by_default() {
        let args = RunRustJanusSearchArgs::try_parse_from([
            "run-rust-janus-search",
            "--workdir",
            "scratch/janus",
            "--run-dir",
            "runs/active/janus",
        ])
        .expect("args");

        assert!(args.atoms_in_template.is_none());
    }

    #[test]
    fn compare_duplicate_edge_cases_cli_leaves_atoms_override_unset_by_default() {
        let args = CompareDuplicateEdgeCasesArgs::try_parse_from(["compare-duplicate-edge-cases"])
            .expect("args");

        assert!(args.atoms_in_template.is_none());
    }

    #[test]
    fn compare_duplicate_edge_cases_cli_accepts_explicit_atoms_override() {
        let args = CompareDuplicateEdgeCasesArgs::try_parse_from([
            "compare-duplicate-edge-cases",
            "--atoms-in-template",
            "inputs/scott/atoms.in",
        ])
        .expect("args");

        assert_eq!(
            args.atoms_in_template,
            Some(PathBuf::from("inputs/scott/atoms.in"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn score_ga_emulate_command_writes_summary_and_artifacts() {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;

        fn candidate(label: &str, offset: f64) -> Candidate {
            Candidate {
                species: vec!["Mg".into(), "O".into()],
                fractional_coords: vec![[0.0 + offset, 0.0, 0.0], [0.5 + offset, 0.5, 0.5]],
                lattice: None,
                periodic_axes: [false, false, false],
                label: label.into(),
            }
        }

        let temp = tempfile::tempdir().expect("tempdir");
        let ga_run_dir = temp.path().join("ga_run");
        let raw_dir = ga_run_dir.join("raw");
        fs::create_dir_all(&raw_dir).expect("raw dir");
        let state = patina_types::GaGenerationState {
            generation: 1,
            population: vec![
                patina_types::GaMemberState {
                    member_id: 0,
                    origin: "MUTATE".into(),
                    occurrences: 1,
                    source: patina_types::StructureRecord::from(&candidate("source-a", 0.0)),
                    evaluation: patina_types::EvaluationRecord::from(&patina_types::EvalResult {
                        energy: -1.0,
                        forces: vec![[0.0, 0.0, 0.0]; 2],
                        relaxed_candidate: candidate("a", 0.1),
                        converged: true,
                        wall_time: Duration::from_secs(0),
                    }),
                    lineage: None,
                    topology: patina_types::GaMemberTopologyRecord::default(),
                },
                patina_types::GaMemberState {
                    member_id: 1,
                    origin: "MUTATE".into(),
                    occurrences: 1,
                    source: patina_types::StructureRecord::from(&candidate("source-b", 0.2)),
                    evaluation: patina_types::EvaluationRecord::from(&patina_types::EvalResult {
                        energy: -2.0,
                        forces: vec![[0.0, 0.0, 0.0]; 2],
                        relaxed_candidate: candidate("b", 0.2),
                        converged: true,
                        wall_time: Duration::from_secs(0),
                    }),
                    lineage: None,
                    topology: patina_types::GaMemberTopologyRecord::default(),
                },
                patina_types::GaMemberState {
                    member_id: 2,
                    origin: "MUTATE".into(),
                    occurrences: 1,
                    source: patina_types::StructureRecord::from(&candidate("source-c", 0.3)),
                    evaluation: patina_types::EvaluationRecord::from(&patina_types::EvalResult {
                        energy: -3.0,
                        forces: vec![[0.0, 0.0, 0.0]; 2],
                        relaxed_candidate: candidate("c", 0.3),
                        converged: true,
                        wall_time: Duration::from_secs(0),
                    }),
                    lineage: None,
                    topology: patina_types::GaMemberTopologyRecord::default(),
                },
            ],
            elites: Vec::new(),
            repopulation: Vec::new(),
        };
        fs::write(
            raw_dir.join("generation_0001_state.json"),
            serde_json::to_string_pretty(&state).expect("serialize state"),
        )
        .expect("write state");

        let pending_path = temp.path().join("pending.xyz");
        fs::write(&pending_path, "2\ncomment\nMg 0.1 0.0 0.0\nO 0.6 0.5 0.5\n")
            .expect("write pending xyz");

        let project_dir = temp.path().join("fake_project");
        fs::create_dir_all(&project_dir).expect("project dir");
        let uv_bin = temp.path().join("fake_uv.sh");
        let mut script = fs::File::create(&uv_bin).expect("fake uv");
        writeln!(
            script,
            "#!/bin/sh\nREQUEST=\"\"\nRESPONSE=\"\"\nwhile [ \"$#\" -gt 0 ]; do\n  case \"$1\" in\n    --request) REQUEST=\"$2\"; shift 2 ;;\n    --response) RESPONSE=\"$2\"; shift 2 ;;\n    *) shift ;;\n  esac\ndone\ncat > \"$RESPONSE\" <<'JSON'\n{{\n  \"schema_version\": \"{}\",\n  \"campaign_id\": \"campaign-test\",\n  \"branch_id\": \"branch-test\",\n  \"workflow\": \"genetic_algorithm\",\n  \"task\": \"score_candidates\",\n  \"direction\": \"downstream\",\n  \"objective\": \"reduce_uncertainty\",\n  \"feature_names\": [\"atom_count\", \"species_count\", \"periodic_dimension\", \"centroid_x\", \"centroid_y\", \"centroid_z\", \"span_x\", \"span_y\", \"span_z\", \"mean_radius\", \"rms_radius\", \"min_pair_distance\", \"mean_pair_distance\", \"max_pair_distance\", \"lattice_volume\", \"species_count:Mg\", \"species_count:O\"],\n  \"target_names\": [\"energy\"],\n  \"model_variant\": \"whitened_svgp\",\n  \"selected_model_name\": \"Whitened Full-Covariance SVGP\",\n  \"incumbent_target\": -3.0,\n  \"predictions\": [\n    {{\n      \"candidate_id\": \"ga_pending_0000\",\n      \"means\": [-2.7],\n      \"variances\": [0.12],\n      \"acquisition_score\": 0.9,\n      \"uncertainty_score\": 0.4,\n      \"rank\": 1\n    }}\n  ]\n}}\nJSON\n",
            patina_emulate::SURROGATE_RESPONSE_SCHEMA_VERSION
        )
        .expect("write script");
        drop(script);
        let mut permissions = fs::metadata(&uv_bin).expect("metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&uv_bin, permissions).expect("chmod");

        let output_dir = temp.path().join("surrogate_out");
        run_score_ga_emulate(ScoreGaEmulateArgs {
            ga_run_dir,
            output_dir: output_dir.clone(),
            pending_candidate: vec![pending_path],
            pending_candidate_dir: None,
            campaign_id: Some("campaign-test".into()),
            branch_id: "branch-test".into(),
            objective: EmulateLearningObjectiveCli::ReduceUncertainty,
            fidelity: EmulateFidelityCli::JanusMaceLow,
            feature_projector: EmulateFeatureProjectorCli::SimpleStructureStatistics,
            target_name: "energy".into(),
            target_unit: "eV".into(),
            model_variant: SurrogateModelVariantCli::Whitened,
            surrogate_objective: SurrogateObjectiveCli::Minimize,
            epochs: 10,
            num_inducing: 4,
            batch_size: 4,
            lr: 0.05,
            n_bootstraps: 1,
            random_seed: 7,
            log_level: "warning".into(),
            uv_bin: Some(uv_bin),
            emulate_project_dir: Some(project_dir),
            emulate_environment: "autoemulate".into(),
            timeout_secs: 30,
        })
        .expect("run score-ga-emulate");

        let artifact_dir = application::driver_support::latest_surrogate_artifact_dir(&output_dir)
            .expect("artifact dir search")
            .expect("artifact dir");
        assert!(artifact_dir.join("request.json").exists());
        assert!(artifact_dir.join("response.json").exists());
        assert!(artifact_dir.join("training_observations.json").exists());
        assert!(artifact_dir.join("acquisition_records.json").exists());
        assert!(artifact_dir.join("emulate_summary.json").exists());
        assert!(artifact_dir.join("emulate_report.json").exists());
    }
}
