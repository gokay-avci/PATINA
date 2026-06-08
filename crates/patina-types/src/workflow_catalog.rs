/// High-level launch context for a workflow definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowContext {
    StartsNewRun,
    UsesCurrentRun,
    UsesSelectedStructure,
}

impl WorkflowContext {
    pub fn label(self) -> &'static str {
        match self {
            Self::StartsNewRun => "new-run",
            Self::UsesCurrentRun => "follow-on",
            Self::UsesSelectedStructure => "structure-tool",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::StartsNewRun => "Creates a new run directory with full workflow artifacts.",
            Self::UsesCurrentRun => "Consumes an existing run as upstream context.",
            Self::UsesSelectedStructure => {
                "Operates from a selected structure input without full run replay."
            }
        }
    }
}

/// Contract bucket for one file-shaped part of a workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowFileKind {
    ScientificInput,
    RuntimeSupport,
    AdapterTemplate,
    GeneratedArtifact,
}

impl WorkflowFileKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::ScientificInput => "scientific-input",
            Self::RuntimeSupport => "runtime-support",
            Self::AdapterTemplate => "adapter-template",
            Self::GeneratedArtifact => "generated-artifact",
        }
    }
}

/// One file or file-pattern contract attached to a workflow family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkflowFileContract {
    pub label: &'static str,
    pub kind: WorkflowFileKind,
    pub path_pattern: &'static str,
    pub required: bool,
    pub detail: &'static str,
}

/// Shared workflow definition consumed by CLI, TUI, and app-oriented surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkflowDefinition {
    pub id: &'static str,
    pub name: &'static str,
    pub route: &'static str,
    pub family: &'static str,
    pub summary: &'static str,
    pub when_to_use: &'static str,
    pub inbound_port: &'static str,
    pub adapter: &'static str,
    pub upstream: &'static str,
    pub downstream: &'static str,
    pub required_inputs: &'static [&'static str],
    pub example_args: &'static [&'static str],
    pub key_switches: &'static [&'static str],
    pub outputs: &'static [&'static str],
    pub file_contracts: &'static [WorkflowFileContract],
    pub context: WorkflowContext,
    pub expected_owner: Option<&'static str>,
    pub expected_backend: Option<&'static str>,
}

const GA_OUTPUTS: &[&str] = &[
    "manifest.json",
    "raw/generation_XXXX_state.json",
    "traces/generation_metrics.csv",
    "traces/controller_trace.csv",
    "raw/rust_ga_checkpoint_latest.json",
];

const STAGED_GA_FILES: &[WorkflowFileContract] = &[
    WorkflowFileContract {
        label: "Base candidate or checkpoint",
        kind: WorkflowFileKind::ScientificInput,
        path_pattern: "<candidate.json> | <checkpoint.json>",
        required: true,
        detail: "Seed structure for a fresh run or resumable GA state for continuation.",
    },
    WorkflowFileContract {
        label: "Scott run.job template",
        kind: WorkflowFileKind::AdapterTemplate,
        path_pattern: "inputs/run.job",
        required: true,
        detail: "Stage-level Scott procedure contract used by the staged runtime adapter.",
    },
    WorkflowFileContract {
        label: "GULP master template",
        kind: WorkflowFileKind::AdapterTemplate,
        path_pattern: "inputs/Master.gin",
        required: true,
        detail: "Primary evaluator template for GULP-backed stages.",
    },
    WorkflowFileContract {
        label: "Scott atoms sidecar",
        kind: WorkflowFileKind::AdapterTemplate,
        path_pattern: "inputs/atoms.in",
        required: false,
        detail: "Optional Scott-side atom specification sidecar when native parity needs it.",
    },
    WorkflowFileContract {
        label: "Run manifest",
        kind: WorkflowFileKind::GeneratedArtifact,
        path_pattern: "manifest.json",
        required: true,
        detail: "Canonical run identity, workflow owner, backend policy, and artifact map.",
    },
    WorkflowFileContract {
        label: "GA checkpoint",
        kind: WorkflowFileKind::GeneratedArtifact,
        path_pattern: "raw/rust_ga_checkpoint_latest.json",
        required: true,
        detail: "Resumable Rust-owned GA controller state.",
    },
    WorkflowFileContract {
        label: "Latest generation state",
        kind: WorkflowFileKind::GeneratedArtifact,
        path_pattern: "raw/ga_generation_state_latest.json",
        required: true,
        detail: "Current population snapshot, energies, topology fields, and lineage details.",
    },
    WorkflowFileContract {
        label: "Generation metrics",
        kind: WorkflowFileKind::GeneratedArtifact,
        path_pattern: "traces/generation_metrics.csv",
        required: true,
        detail: "Per-generation health and duplicate-pressure tracking.",
    },
    WorkflowFileContract {
        label: "Controller trace",
        kind: WorkflowFileKind::GeneratedArtifact,
        path_pattern: "traces/controller_trace.csv",
        required: true,
        detail: "Controller-level event trace for auditing GA state transitions.",
    },
    WorkflowFileContract {
        label: "Procedure trace",
        kind: WorkflowFileKind::GeneratedArtifact,
        path_pattern: "raw/staged_scott_procedure_trace.json",
        required: false,
        detail: "Per-request staged evaluator procedure evidence when emitted.",
    },
];

const JANUS_GA_FILES: &[WorkflowFileContract] = &[
    WorkflowFileContract {
        label: "Base candidate or checkpoint",
        kind: WorkflowFileKind::ScientificInput,
        path_pattern: "<candidate.json> | <checkpoint.json>",
        required: true,
        detail: "Seed structure for a fresh run or resumable GA state for continuation.",
    },
    WorkflowFileContract {
        label: "Run manifest",
        kind: WorkflowFileKind::GeneratedArtifact,
        path_pattern: "manifest.json",
        required: true,
        detail: "Canonical run identity, Janus worker posture, and artifact map.",
    },
    WorkflowFileContract {
        label: "GA checkpoint",
        kind: WorkflowFileKind::GeneratedArtifact,
        path_pattern: "raw/rust_ga_checkpoint_latest.json",
        required: true,
        detail: "Resumable Rust-owned GA controller state.",
    },
    WorkflowFileContract {
        label: "Latest generation state",
        kind: WorkflowFileKind::GeneratedArtifact,
        path_pattern: "raw/ga_generation_state_latest.json",
        required: true,
        detail: "Current population snapshot, energies, topology fields, and lineage details.",
    },
    WorkflowFileContract {
        label: "Generation metrics",
        kind: WorkflowFileKind::GeneratedArtifact,
        path_pattern: "traces/generation_metrics.csv",
        required: true,
        detail: "Per-generation health and duplicate-pressure tracking.",
    },
    WorkflowFileContract {
        label: "Controller trace",
        kind: WorkflowFileKind::GeneratedArtifact,
        path_pattern: "traces/controller_trace.csv",
        required: true,
        detail: "Controller-level event trace for auditing GA state transitions.",
    },
    WorkflowFileContract {
        label: "Search summary",
        kind: WorkflowFileKind::GeneratedArtifact,
        path_pattern: "raw/search_summary.json",
        required: false,
        detail: "Convenience summary for downstream inspection surfaces when present.",
    },
];

const LEGACY_SCOTT_FILES: &[WorkflowFileContract] = &[
    WorkflowFileContract {
        label: "Native SCOTT data directory",
        kind: WorkflowFileKind::RuntimeSupport,
        path_pattern: "<scott-runtime-dir>",
        required: true,
        detail: "Native runtime tree consumed by the preserved SCOTT executable pathway.",
    },
    WorkflowFileContract {
        label: "Native SCOTT executable",
        kind: WorkflowFileKind::RuntimeSupport,
        path_pattern: "<klmc_scott>",
        required: true,
        detail: "Native SCOTT binary used for the preserved search route.",
    },
    WorkflowFileContract {
        label: "Native run manifest",
        kind: WorkflowFileKind::GeneratedArtifact,
        path_pattern: "manifest.json",
        required: true,
        detail: "Tracked Rust-side manifest for the preserved native execution path.",
    },
    WorkflowFileContract {
        label: "Exported structures",
        kind: WorkflowFileKind::GeneratedArtifact,
        path_pattern: "outputs/structures/*",
        required: false,
        detail: "Tracked exported structures recovered from the native run output tree.",
    },
    WorkflowFileContract {
        label: "Native summaries",
        kind: WorkflowFileKind::GeneratedArtifact,
        path_pattern: "raw/top_structures_* | raw/native_*",
        required: false,
        detail: "Preserved native summary artifacts when export capture succeeds.",
    },
];

const BASIN_HOPPING_FILES: &[WorkflowFileContract] = &[
    WorkflowFileContract {
        label: "Seed candidate",
        kind: WorkflowFileKind::ScientificInput,
        path_pattern: "<candidate.json>",
        required: true,
        detail: "Zero-dimensional or periodic candidate used to seed the BH walker set.",
    },
    WorkflowFileContract {
        label: "Backend template bundle",
        kind: WorkflowFileKind::AdapterTemplate,
        path_pattern: "inputs/Master.gin | inputs/run.job | inputs/jobs/*",
        required: false,
        detail: "Optional backend-specific templates when BH is routed through Scott/GULP-style adapters.",
    },
    WorkflowFileContract {
        label: "Run manifest",
        kind: WorkflowFileKind::GeneratedArtifact,
        path_pattern: "manifest.json",
        required: true,
        detail: "Canonical run identity and BH scientific configuration.",
    },
    WorkflowFileContract {
        label: "Walker trace",
        kind: WorkflowFileKind::GeneratedArtifact,
        path_pattern: "traces/walker_trace.csv",
        required: true,
        detail: "Accepted/rejected BH walker history for monitoring and audit.",
    },
    WorkflowFileContract {
        label: "BH summary",
        kind: WorkflowFileKind::GeneratedArtifact,
        path_pattern: "raw/basin_hopping_summary.json",
        required: false,
        detail: "Compact summary of basin-hopping execution when emitted.",
    },
];

const HYBRID_GA_PRODUCTION_FILES: &[WorkflowFileContract] = &[
    WorkflowFileContract {
        label: "Base candidate",
        kind: WorkflowFileKind::ScientificInput,
        path_pattern: "<candidate.json>",
        required: true,
        detail: "Seed candidate for the initial GA portion of the hybrid workflow.",
    },
    WorkflowFileContract {
        label: "Scott template bundle",
        kind: WorkflowFileKind::AdapterTemplate,
        path_pattern: "inputs/run.job | inputs/Master.gin | inputs/atoms.in",
        required: false,
        detail: "Adapter templates required when the downstream production stages use Scott/GULP semantics.",
    },
    WorkflowFileContract {
        label: "Top-level hybrid manifest",
        kind: WorkflowFileKind::GeneratedArtifact,
        path_pattern: "manifest.json",
        required: true,
        detail: "Hybrid workflow identity plus nested GA and production artifact ownership.",
    },
    WorkflowFileContract {
        label: "Nested GA tree",
        kind: WorkflowFileKind::GeneratedArtifact,
        path_pattern: "ga/*",
        required: false,
        detail: "Embedded or sibling GA artifact tree produced before seed promotion.",
    },
    WorkflowFileContract {
        label: "Nested production tree",
        kind: WorkflowFileKind::GeneratedArtifact,
        path_pattern: "production/*",
        required: false,
        detail: "Embedded or sibling production artifact tree produced after promotion.",
    },
];

const ENERGY_LID_FILES: &[WorkflowFileContract] = &[
    WorkflowFileContract {
        label: "Source GA run manifest",
        kind: WorkflowFileKind::ScientificInput,
        path_pattern: "<source-run-dir>/manifest.json",
        required: true,
        detail: "Upstream GA identity used to bind lid exploration to a tracked source run.",
    },
    WorkflowFileContract {
        label: "Source GA generation states",
        kind: WorkflowFileKind::ScientificInput,
        path_pattern: "<source-run-dir>/raw/generation_XXXX_state.json",
        required: true,
        detail: "Population and ranking evidence used to choose lid seeds.",
    },
    WorkflowFileContract {
        label: "Backend template bundle",
        kind: WorkflowFileKind::AdapterTemplate,
        path_pattern: "inputs/Master.gin | inputs/run.job | inputs/jobs/*",
        required: false,
        detail:
            "Optional backend-specific templates when lid sampling uses Scott/GULP-style adapters.",
    },
    WorkflowFileContract {
        label: "Run manifest",
        kind: WorkflowFileKind::GeneratedArtifact,
        path_pattern: "manifest.json",
        required: true,
        detail: "Canonical follow-on run identity plus lid configuration.",
    },
    WorkflowFileContract {
        label: "Energy-lid summary",
        kind: WorkflowFileKind::GeneratedArtifact,
        path_pattern: "raw/energy_lid_summary.json",
        required: true,
        detail: "Windowed lid execution summary for each promoted source structure.",
    },
    WorkflowFileContract {
        label: "Lid runner traces",
        kind: WorkflowFileKind::GeneratedArtifact,
        path_pattern: "traces/energy_lid/*/mc_trace.csv",
        required: false,
        detail: "Per-runner Monte Carlo traces when trace export is enabled.",
    },
];

const ANNEALING_FILES: &[WorkflowFileContract] = &[
    WorkflowFileContract {
        label: "Source GA run manifest",
        kind: WorkflowFileKind::ScientificInput,
        path_pattern: "<source-run-dir>/manifest.json",
        required: true,
        detail: "Upstream GA identity used to bind annealing to a tracked source run.",
    },
    WorkflowFileContract {
        label: "Source GA generation states",
        kind: WorkflowFileKind::ScientificInput,
        path_pattern: "<source-run-dir>/raw/generation_XXXX_state.json",
        required: true,
        detail: "Population and ranking evidence used to choose annealing seeds.",
    },
    WorkflowFileContract {
        label: "Backend template bundle",
        kind: WorkflowFileKind::AdapterTemplate,
        path_pattern: "inputs/Master.gin | inputs/run.job | inputs/jobs/*",
        required: false,
        detail:
            "Optional backend-specific templates when annealing uses Scott/GULP-style adapters.",
    },
    WorkflowFileContract {
        label: "Run manifest",
        kind: WorkflowFileKind::GeneratedArtifact,
        path_pattern: "manifest.json",
        required: true,
        detail: "Canonical follow-on run identity plus annealing schedule.",
    },
    WorkflowFileContract {
        label: "Annealing summary",
        kind: WorkflowFileKind::GeneratedArtifact,
        path_pattern: "raw/simulated_annealing_summary.json",
        required: true,
        detail: "Annealing and quench execution summary for each promoted source structure.",
    },
    WorkflowFileContract {
        label: "Annealing traces",
        kind: WorkflowFileKind::GeneratedArtifact,
        path_pattern: "traces/simulated_annealing/*/mc_trace.csv",
        required: false,
        detail: "Per-runner Monte Carlo traces when trace export is enabled.",
    },
];

const PERTURB_CLUSTER_FILES: &[WorkflowFileContract] = &[
    WorkflowFileContract {
        label: "Seed candidate",
        kind: WorkflowFileKind::ScientificInput,
        path_pattern: "<candidate.json>",
        required: true,
        detail: "Input cluster used for perturbation, duplicate screening, and export.",
    },
    WorkflowFileContract {
        label: "Run manifest",
        kind: WorkflowFileKind::GeneratedArtifact,
        path_pattern: "manifest.json",
        required: true,
        detail: "Structure-tool run identity and perturbation settings.",
    },
    WorkflowFileContract {
        label: "Perturbed candidates",
        kind: WorkflowFileKind::GeneratedArtifact,
        path_pattern: "outputs/perturbations/*",
        required: false,
        detail: "Generated candidate exports retained after duplicate-aware filtering.",
    },
    WorkflowFileContract {
        label: "Duplicate report",
        kind: WorkflowFileKind::GeneratedArtifact,
        path_pattern: "raw/duplicate_screening.json",
        required: false,
        detail: "Duplicate and fingerprint decisions recorded for the perturbation batch.",
    },
];

const SURFACE_GENERATION_FILES: &[WorkflowFileContract] = &[
    WorkflowFileContract {
        label: "Periodic structure input",
        kind: WorkflowFileKind::ScientificInput,
        path_pattern: "<candidate.json> | <structure.cif> | <structure.xyz>",
        required: true,
        detail: "Framework or slab parent structure used to derive the requested surface.",
    },
    WorkflowFileContract {
        label: "Run manifest",
        kind: WorkflowFileKind::GeneratedArtifact,
        path_pattern: "manifest.json",
        required: true,
        detail: "Surface-generation identity, Miller index, and cut/reduction settings.",
    },
    WorkflowFileContract {
        label: "Surface slab artifacts",
        kind: WorkflowFileKind::GeneratedArtifact,
        path_pattern: "outputs/surfaces/*",
        required: false,
        detail: "Exported slab structures and companion diagnostics.",
    },
    WorkflowFileContract {
        label: "Surface diagnostics",
        kind: WorkflowFileKind::GeneratedArtifact,
        path_pattern: "raw/surface_*",
        required: false,
        detail: "Cut offset, topology diagnostics, and polarity-related warnings.",
    },
];

const FRAMEWORK_GCMC_FILES: &[WorkflowFileContract] = &[
    WorkflowFileContract {
        label: "Framework structure input",
        kind: WorkflowFileKind::ScientificInput,
        path_pattern: "<candidate.json> | <structure.cif>",
        required: true,
        detail: "Periodic framework supplied to the adsorption/GCMC workflow.",
    },
    WorkflowFileContract {
        label: "Run manifest",
        kind: WorkflowFileKind::GeneratedArtifact,
        path_pattern: "manifest.json",
        required: true,
        detail: "Framework GCMC identity, thermodynamic settings, and backend metadata.",
    },
    WorkflowFileContract {
        label: "Framework GCMC artifacts",
        kind: WorkflowFileKind::GeneratedArtifact,
        path_pattern: "outputs/gcmc/* | raw/gcmc_*",
        required: false,
        detail: "Adsorption-oriented outputs and backend summaries produced by the workflow.",
    },
];

const WORKFLOW_REGISTRY: &[WorkflowDefinition] = &[
    WorkflowDefinition {
        id: "ga.scott-monolithic",
        name: "GA: Scott Monolithic Runtime",
        route: "run-ga scott-monolithic",
        family: "Rust-owned GA",
        summary: "Shared Rust GA controller with monolithic Scott runtime routing across concrete backends.",
        when_to_use: "Choose this when you want the mature Rust GA controller with Scott-shaped monolithic runtime semantics and explicit backend routing.",
        inbound_port: "GaWorkflowService -> GaEvaluationPort",
        adapter: "MonolithicScottGaEvaluator -> RoutedBackendStageExecutor",
        upstream: "Base candidate, Scott templates or input dir, runtime routing policy, GA controller parameters.",
        downstream: "Tracked GA artifacts, generation snapshots, controller trace, checkpoint resume path.",
        required_inputs: &[
            "--run-dir <new-run-dir>",
            "--workdir <sandbox-workdir>",
            "--base-candidate-json <candidate.json> or --resume-from-checkpoint <checkpoint.json>",
        ],
        example_args: &[
            "--ga-generations 20",
            "--population 24",
            "--runtime-default-backend gulp",
            "--runtime-stage-backend 2=janus",
            "--seed 11",
        ],
        key_switches: &[
            "--ga-generations",
            "--population",
            "--runtime-default-backend",
            "--runtime-stage-backend",
            "--scott-input-dir",
            "--seed",
        ],
        outputs: GA_OUTPUTS,
        file_contracts: STAGED_GA_FILES,
        context: WorkflowContext::StartsNewRun,
        expected_owner: Some("scott_monolithic_ga"),
        expected_backend: Some("scott_runtime"),
    },
    WorkflowDefinition {
        id: "ga.persistent-daemon",
        name: "GA: Persistent Daemon Workers",
        route: "run-ga persistent-daemon",
        family: "Rust-owned GA",
        summary: "Shared Rust GA controller with long-lived daemon worker processes and stable per-worker sandboxes.",
        when_to_use: "Choose this when the Janus/MACE backend should stay warm across generations and worker parallelism matters more than Scott monolithic parity.",
        inbound_port: "GaWorkflowService -> GaEvaluationPort",
        adapter: "PersistentPoolGaEvaluator -> PersistentWorkerPool",
        upstream: "Base candidate, worker count, Janus backend configuration, duplicate policy settings.",
        downstream: "Tracked GA artifacts, persistent-worker metadata, generation snapshots, checkpoint resume path.",
        required_inputs: &[
            "--run-dir <new-run-dir>",
            "--workdir <sandbox-workdir>",
            "--base-candidate-json <candidate.json> or --resume-from-checkpoint <checkpoint.json>",
        ],
        example_args: &[
            "--ga-generations 20",
            "--population 24",
            "--workers 4",
            "--janus-model small",
            "--janus-device cpu",
        ],
        key_switches: &[
            "--ga-generations",
            "--population",
            "--workers",
            "--janus-model",
            "--janus-device",
            "--duplicate-policy-mode",
        ],
        outputs: GA_OUTPUTS,
        file_contracts: JANUS_GA_FILES,
        context: WorkflowContext::StartsNewRun,
        expected_owner: Some("persistent_daemon_ga"),
        expected_backend: Some("janus_mace"),
    },
    WorkflowDefinition {
        id: "legacy.scott-search",
        name: "Legacy Scott Search",
        route: "legacy-scott",
        family: "Native SCOTT-owned search",
        summary: "Preserved native `klmc_scott` execution path exported through the Rust tracking layer.",
        when_to_use: "Choose this when you need the native SCOTT workflow exactly as owned upstream, with tracked export rather than Rust-owned control; BH and GA are the characterized typed-trace modes, solid-solutions and testing are characterized for generic native export, and the remaining native entry points should still be treated as launch-only until their fixture decks are characterized.",
        inbound_port: "LegacyScottExecutionPort",
        adapter: "Native SCOTT binary + preserved export bridge",
        upstream: "Native SCOTT config, runtime data dir, evaluator backend, and native job-type selection.",
        downstream: "Tracked native artifacts, exported structures, native-compatible provenance for parity work, and explicit support-tier boundaries for each preserved native entry point.",
        required_inputs: &[
            "--run-dir <new-run-dir>",
            "--workdir <sandbox-workdir>",
            "--data-dir <scott-runtime-dir>",
            "--scott-bin <klmc_scott>",
        ],
        example_args: &[
            "--job-type genetic-algorithm",
            "--population 24",
            "--ga-generations 20",
            "--evaluator-backend gulp",
            "--temperature 10.0",
        ],
        key_switches: &[
            "--job-type <native-entrypoint>",
            "--mode ga|bh (legacy shorthand)",
            "--ga-generations / --bh-steps",
            "--population",
            "--evaluator-backend",
            "--temperature",
            "--seed",
        ],
        outputs: &[
            "manifest.json",
            "raw/top_structures_* or native summaries",
            "outputs/structures/*",
            "tracked native metadata",
        ],
        file_contracts: LEGACY_SCOTT_FILES,
        context: WorkflowContext::StartsNewRun,
        expected_owner: None,
        expected_backend: None,
    },
    WorkflowDefinition {
        id: "sampling.basin-hopping",
        name: "Basin Hopping",
        route: "run-basin-hopping",
        family: "Rust-owned BH",
        summary: "Typed basin-hopping controller over the shared backend evaluation seam.",
        when_to_use: "Choose this when you want a direct sampling workflow from one seed cluster instead of a population-driven GA run.",
        inbound_port: "Sampling workflow request -> backend evaluation port",
        adapter: "Selected backend adapter behind EvalBackendKind",
        upstream: "Seed candidate JSON, walker count, BH temperature/step size, backend choice.",
        downstream: "Walker trace, accepted/rejected history, run summary, tracked artifacts.",
        required_inputs: &[
            "--candidate-json <candidate.json>",
            "--run-dir <new-run-dir>",
            "--workdir <sandbox-workdir>",
        ],
        example_args: &[
            "--backend gulp",
            "--bh-steps 40",
            "--walkers 1",
            "--temperature 10.0",
            "--step-size 0.1",
        ],
        key_switches: &[
            "--backend",
            "--bh-steps",
            "--walkers",
            "--temperature",
            "--step-size",
            "--boundary",
        ],
        outputs: &[
            "manifest.json",
            "traces/walker_trace.csv",
            "outputs/final structures",
            "tracked BH summary",
        ],
        file_contracts: BASIN_HOPPING_FILES,
        context: WorkflowContext::StartsNewRun,
        expected_owner: Some("sampling_basin_hopping"),
        expected_backend: None,
    },
    WorkflowDefinition {
        id: "hybrid.ga-production",
        name: "Hybrid: GA to Production",
        route: "run-hybrid-ga-production",
        family: "Rust-owned staged workflow",
        summary: "Staged GA exploration followed by production-style evaluation, with explicit promotion of selected GA seeds.",
        when_to_use: "Choose this when you need search plus downstream production in one tracked pipeline rather than two disconnected runs.",
        inbound_port: "Hybrid workflow orchestrator over GA and production services",
        adapter: "Shared GA service + staged production runtime adapters",
        upstream: "Base candidate, staged runtime config, seed selection policy, backend routing.",
        downstream: "GA artifacts, production artifacts, promotion trace, optional emulate-related outputs.",
        required_inputs: &[
            "--run-dir <new-run-dir>",
            "--workdir <sandbox-workdir>",
            "--base-candidate-json <candidate.json>",
        ],
        example_args: &[
            "--ga-generations 20",
            "--population 24",
            "--max-production-seeds 5",
            "--seed-selection-mode best-energy",
            "--runtime-default-backend gulp",
        ],
        key_switches: &[
            "--ga-generations",
            "--population",
            "--max-production-seeds",
            "--seed-selection-mode",
            "--runtime-default-backend",
            "--runtime-stage-backend",
        ],
        outputs: &[
            "top-level hybrid manifest",
            "nested GA and production artifact trees",
            "promotion metadata",
            "staged runtime traces",
        ],
        file_contracts: HYBRID_GA_PRODUCTION_FILES,
        context: WorkflowContext::StartsNewRun,
        expected_owner: None,
        expected_backend: None,
    },
    WorkflowDefinition {
        id: "sampling.energy-lid",
        name: "Energy Lid from Current GA Run",
        route: "run-energy-lid",
        family: "Follow-on workflow",
        summary: "Starts from top structures in an existing tracked GA run and explores energy-lid windows.",
        when_to_use: "Choose this after a GA run when you want controlled lid exploration around top-ranked structures.",
        inbound_port: "Tracked GA artifact reader -> energy-lid controller",
        adapter: "Selected backend adapter behind EvalBackendKind",
        upstream: "Current GA run, top-N structure selection, lid schedule, backend choice.",
        downstream: "Energy-lid run directory, lid summaries, runner/quench outputs, follow-on diagnostics.",
        required_inputs: &[
            "--source-run-dir <current-ga-run>",
            "--run-dir <new-run-dir>",
            "--workdir <sandbox-workdir>",
        ],
        example_args: &[
            "--backend gulp",
            "--top-n 8",
            "--lid-levels 8",
            "--lid-increment 1.0",
            "--steps-per-lid 20",
        ],
        key_switches: &[
            "--backend",
            "--top-n",
            "--lid-levels",
            "--lid-increment",
            "--steps-per-lid",
            "--runners-per-lid",
        ],
        outputs: &[
            "manifest.json",
            "energy-lid summaries",
            "per-lid runner outputs",
            "tracked follow-on artifacts",
        ],
        file_contracts: ENERGY_LID_FILES,
        context: WorkflowContext::UsesCurrentRun,
        expected_owner: Some("sampling_energy_lid"),
        expected_backend: None,
    },
    WorkflowDefinition {
        id: "sampling.simulated-annealing",
        name: "Simulated Annealing from Current GA Run",
        route: "run-simulated-annealing",
        family: "Follow-on workflow",
        summary: "Starts from top structures in an existing tracked GA run and performs controlled annealing plus quench passes.",
        when_to_use: "Choose this after a GA run when you want temperature-controlled sampling around the best discovered structures.",
        inbound_port: "Tracked GA artifact reader -> annealing controller",
        adapter: "Selected backend adapter behind EvalBackendKind",
        upstream: "Current GA run, top-N structure selection, annealing schedule, backend choice.",
        downstream: "Annealing run directory, quench summaries, tracked temperature history.",
        required_inputs: &[
            "--source-run-dir <current-ga-run>",
            "--run-dir <new-run-dir>",
            "--workdir <sandbox-workdir>",
        ],
        example_args: &[
            "--backend gulp",
            "--top-n 8",
            "--anneal-steps 40",
            "--initial-temperature 25.0",
            "--temperature-scale 0.8",
        ],
        key_switches: &[
            "--backend",
            "--top-n",
            "--anneal-steps",
            "--initial-temperature",
            "--temperature-scale",
            "--quench-steps",
        ],
        outputs: &[
            "manifest.json",
            "annealing summaries",
            "quench outputs",
            "tracked follow-on artifacts",
        ],
        file_contracts: ANNEALING_FILES,
        context: WorkflowContext::UsesCurrentRun,
        expected_owner: Some("sampling_simulated_annealing"),
        expected_backend: None,
    },
    WorkflowDefinition {
        id: "structure.perturb-cluster",
        name: "Perturb Cluster",
        route: "perturb-cluster",
        family: "Structure tool",
        summary: "Generates perturbed zero-dimensional cluster variants with duplicate-aware filtering.",
        when_to_use: "Choose this when you want a playground for candidate diversification, duplicate probes, or seed generation outside a full search run.",
        inbound_port: "Candidate input -> perturbation / fingerprint / duplicate ports",
        adapter: "Rust perturbation and duplicate-classification adapters",
        upstream: "Seed candidate JSON, perturbation sigma, duplicate threshold, optional distance validation.",
        downstream: "Perturbed variants, duplicate-screening report, future seed bundles for other workflows.",
        required_inputs: &[
            "--candidate-json <candidate.json>",
            "--run-dir <new-run-dir>",
            "--count <n>",
        ],
        example_args: &[
            "--count 8",
            "--sigma 0.15",
            "--max-displacement 0.3",
            "--duplicate-threshold 0.02",
            "--seed 17",
        ],
        key_switches: &[
            "--sigma",
            "--max-displacement",
            "--validate-min-distance",
            "--duplicate-threshold",
            "--seed",
        ],
        outputs: &[
            "manifest.json",
            "perturbed candidate artifacts",
            "duplicate-screening metadata",
        ],
        file_contracts: PERTURB_CLUSTER_FILES,
        context: WorkflowContext::UsesSelectedStructure,
        expected_owner: Some("cluster_perturbation"),
        expected_backend: None,
    },
    WorkflowDefinition {
        id: "framework.generate-surface",
        name: "Surface Generation",
        route: "generate-surface",
        family: "Framework tool",
        summary: "Cuts a surface slab from a periodic structure using an explicit crystallographic slab-generation contract.",
        when_to_use: "Choose this when the selected structure is periodic and you want a slab-building step with visible surface assumptions.",
        inbound_port: "Framework/surface generation request",
        adapter: "Rust surface builder with topology-aware or fixed-offset termination selection",
        upstream: "Periodic structure input, Miller indices, thickness, vacuum, and slab-reduction settings.",
        downstream: "Surface slab artifacts, provenance for polarity or downstream adsorption studies.",
        required_inputs: &[
            "--candidate-json <candidate.json> or --structure-path <file>",
            "--run-dir <new-run-dir>",
            "--h <h> --k <k> --l <l>",
        ],
        example_args: &[
            "--h 1 --k 0 --l 0",
            "--thickness 10.0",
            "--vacuum 15.0",
            "--cut-strategy topology-aware",
        ],
        key_switches: &[
            "--thickness",
            "--vacuum",
            "--cut-strategy",
            "--cut-offset-fraction",
        ],
        outputs: &[
            "manifest.json",
            "surface slab artifact set",
            "tracked crystallographic metadata",
        ],
        file_contracts: SURFACE_GENERATION_FILES,
        context: WorkflowContext::UsesSelectedStructure,
        expected_owner: Some("framework_generate_surface"),
        expected_backend: None,
    },
    WorkflowDefinition {
        id: "framework.gcmc",
        name: "Framework GCMC",
        route: "run-framework-gcmc",
        family: "Framework workflow",
        summary: "Runs the Rust-owned framework GCMC entrypoint with periodic Janus/MACE evaluation and RASPA-shaped artifacts.",
        when_to_use: "Choose this when a periodic framework should be evaluated as an adsorption workflow rather than a cluster search.",
        inbound_port: "Framework GCMC execution request",
        adapter: "Periodic Janus/MACE evaluation + RASPA-style artifact adapters",
        upstream: "Framework structure, workdir, guest model, thermodynamic conditions, backend settings.",
        downstream: "Tracked GCMC artifacts, backend summaries, framework-specific diagnostics.",
        required_inputs: &[
            "--candidate-json <candidate.json> or --structure-path <file>",
            "--run-dir <new-run-dir>",
            "--workdir <sandbox-workdir>",
        ],
        example_args: &[
            "--guest h2",
            "--temperature-kelvin 298.0",
            "--pressure-bar 1.0",
            "--janus-model small",
            "--timeout-secs 300",
        ],
        key_switches: &[
            "--guest",
            "--temperature-kelvin",
            "--pressure-bar",
            "--janus-model",
            "--janus-device",
            "--timeout-secs",
        ],
        outputs: &[
            "manifest.json",
            "framework GCMC artifacts",
            "periodic backend summaries",
        ],
        file_contracts: FRAMEWORK_GCMC_FILES,
        context: WorkflowContext::UsesSelectedStructure,
        expected_owner: Some("framework_gcmc"),
        expected_backend: Some("janus_mace"),
    },
];

pub fn workflow_registry() -> &'static [WorkflowDefinition] {
    WORKFLOW_REGISTRY
}

pub fn workflow_by_id(id: &str) -> Option<&'static WorkflowDefinition> {
    let canonical = match id {
        "ga.scott-staged" => "ga.scott-monolithic",
        "ga.janus-persistent" => "ga.persistent-daemon",
        other => other,
    };
    workflow_registry()
        .iter()
        .find(|workflow| workflow.id == canonical)
}

pub fn workflow_by_route(route: &str) -> Option<&'static WorkflowDefinition> {
    let canonical = match route {
        "run-ga scott-staged" => "run-ga scott-monolithic",
        "run-ga janus-persistent" => "run-ga persistent-daemon",
        other => other,
    };
    workflow_registry()
        .iter()
        .find(|workflow| workflow.route == canonical)
}
