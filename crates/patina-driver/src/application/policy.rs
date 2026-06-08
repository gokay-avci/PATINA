use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum WorkflowLane {
    NativeScott,
    RustScottParity,
    RustOwnedJanus,
}

impl WorkflowLane {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NativeScott => "native_scott_search",
            Self::RustScottParity => "rust_scott_parity",
            Self::RustOwnedJanus => "rust_persistent_daemon",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ScientificScope {
    FullSearchOwner,
    ParityWorkflowOwner,
    EvaluatorAndCampaignLayer,
    ExplicitResearchFork,
}

impl ScientificScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FullSearchOwner => "full_search_owner",
            Self::ParityWorkflowOwner => "parity_workflow_owner",
            Self::EvaluatorAndCampaignLayer => "evaluator_and_campaign_layer",
            Self::ExplicitResearchFork => "explicit_research_fork",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SearchFamily {
    ProductionRun,
    GeneticAlgorithm,
    BasinHopping,
    SolidSolutions,
    ScanSurface,
    SimulatedAnnealing,
    EnergyLid,
    HybridGaProduction,
    FutureScottFork,
}

impl SearchFamily {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ProductionRun => "production_run",
            Self::GeneticAlgorithm => "genetic_algorithm",
            Self::BasinHopping => "basin_hopping",
            Self::SolidSolutions => "solid_solutions",
            Self::ScanSurface => "scan_surface",
            Self::SimulatedAnnealing => "simulated_annealing",
            Self::EnergyLid => "energy_lid",
            Self::HybridGaProduction => "hybrid_ga_production",
            Self::FutureScottFork => "future_scott_fork",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ForkReadiness {
    NativeOnly,
    ParityPlanning,
    ParityHardening,
    ResearchOnly,
}

impl ForkReadiness {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NativeOnly => "native_only",
            Self::ParityPlanning => "parity_planning",
            Self::ParityHardening => "parity_hardening",
            Self::ResearchOnly => "research_only",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SearchForkPolicy {
    pub family: SearchFamily,
    pub readiness: ForkReadiness,
    pub native_lane_scope: ScientificScope,
    pub rust_lane_scope: ScientificScope,
}

impl SearchForkPolicy {
    pub const fn for_family(family: SearchFamily) -> Self {
        match family {
            SearchFamily::ProductionRun => Self {
                family,
                readiness: ForkReadiness::ParityHardening,
                native_lane_scope: ScientificScope::FullSearchOwner,
                rust_lane_scope: ScientificScope::ParityWorkflowOwner,
            },
            SearchFamily::GeneticAlgorithm => Self {
                family,
                readiness: ForkReadiness::ParityHardening,
                native_lane_scope: ScientificScope::FullSearchOwner,
                rust_lane_scope: ScientificScope::ExplicitResearchFork,
            },
            SearchFamily::BasinHopping => Self {
                family,
                readiness: ForkReadiness::NativeOnly,
                native_lane_scope: ScientificScope::FullSearchOwner,
                rust_lane_scope: ScientificScope::EvaluatorAndCampaignLayer,
            },
            SearchFamily::SolidSolutions => Self {
                family,
                readiness: ForkReadiness::ParityPlanning,
                native_lane_scope: ScientificScope::FullSearchOwner,
                rust_lane_scope: ScientificScope::ParityWorkflowOwner,
            },
            SearchFamily::ScanSurface => Self {
                family,
                readiness: ForkReadiness::ParityPlanning,
                native_lane_scope: ScientificScope::FullSearchOwner,
                rust_lane_scope: ScientificScope::ParityWorkflowOwner,
            },
            SearchFamily::SimulatedAnnealing => Self {
                family,
                readiness: ForkReadiness::ParityPlanning,
                native_lane_scope: ScientificScope::FullSearchOwner,
                rust_lane_scope: ScientificScope::ParityWorkflowOwner,
            },
            SearchFamily::EnergyLid => Self {
                family,
                readiness: ForkReadiness::ParityPlanning,
                native_lane_scope: ScientificScope::FullSearchOwner,
                rust_lane_scope: ScientificScope::ParityWorkflowOwner,
            },
            SearchFamily::HybridGaProduction => Self {
                family,
                readiness: ForkReadiness::ParityPlanning,
                native_lane_scope: ScientificScope::FullSearchOwner,
                rust_lane_scope: ScientificScope::ParityWorkflowOwner,
            },
            SearchFamily::FutureScottFork => Self {
                family,
                readiness: ForkReadiness::ResearchOnly,
                native_lane_scope: ScientificScope::FullSearchOwner,
                rust_lane_scope: ScientificScope::ExplicitResearchFork,
            },
        }
    }

    pub const fn current() -> [Self; 9] {
        [
            Self::for_family(SearchFamily::ProductionRun),
            Self::for_family(SearchFamily::GeneticAlgorithm),
            Self::for_family(SearchFamily::BasinHopping),
            Self::for_family(SearchFamily::SolidSolutions),
            Self::for_family(SearchFamily::ScanSurface),
            Self::for_family(SearchFamily::SimulatedAnnealing),
            Self::for_family(SearchFamily::EnergyLid),
            Self::for_family(SearchFamily::HybridGaProduction),
            Self::for_family(SearchFamily::FutureScottFork),
        ]
    }
}

pub const PORT_ARCHITECTURE_RULES: &[&str] = &[
    "domain logic stays free of subprocess, filesystem, Tauri, and CLI concerns",
    "application services own workflow coordination and scientific policy checks",
    "adapters implement evaluator, topology identity, persistence, and delivery boundaries",
    "every Rust fork of SCOTT behavior must have explicit parity fixtures before behavior changes",
];
