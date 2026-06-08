use crate::compare_pmoi_scott_intent;
use crate::WorkflowLineage;
use patina_types::{Candidate, EvalResult, SearchConfig};
use std::cmp::Ordering;
use std::str::FromStr;

/// Lifecycle stages used by the parity-oriented Rust GA controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScottGaStage {
    Initialize,
    Selection,
    Crossover,
    Mutation,
    Evaluation,
    Replacement,
    Completed,
}

/// Provenance labels aligned with the native GA bookkeeping vocabulary where possible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScottGaOrigin {
    Seed,
    Crosso,
    Mutate,
    Mutcrs,
    RePopM,
    RePopR,
}

impl ScottGaOrigin {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Seed => "SEED",
            Self::Crosso => "CROSSO",
            Self::Mutate => "MUTATE",
            Self::Mutcrs => "MUTCRS",
            Self::RePopM => "REPOPM",
            Self::RePopR => "REPOPR",
        }
    }

    pub fn parse_label(value: &str) -> Option<Self> {
        Self::from_str(value).ok()
    }
}

impl FromStr for ScottGaOrigin {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "SEED" => Ok(Self::Seed),
            "CROSSO" => Ok(Self::Crosso),
            "MUTATE" => Ok(Self::Mutate),
            "MUTCRS" => Ok(Self::Mutcrs),
            "REPOPM" => Ok(Self::RePopM),
            "REPOPR" => Ok(Self::RePopR),
            _ => Err(()),
        }
    }
}

/// Evaluated member tracked by the parity controller.
#[derive(Debug, Clone, Default)]
pub struct ScottGaTopologyIdentity {
    pub canonical_hashkey: Option<String>,
    pub source_hashkey: Option<String>,
    pub relaxed_hashkey: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ScottGaMember {
    pub source_candidate: Candidate,
    pub result: EvalResult,
    pub origin: ScottGaOrigin,
    pub occurrences: usize,
    pub lineage: WorkflowLineage,
    pub topology: ScottGaTopologyIdentity,
}

impl ScottGaMember {
    pub fn new(
        source_candidate: Candidate,
        result: EvalResult,
        origin: ScottGaOrigin,
        lineage: WorkflowLineage,
    ) -> Self {
        Self {
            source_candidate,
            result,
            origin,
            occurrences: 1,
            lineage,
            topology: ScottGaTopologyIdentity::default(),
        }
    }

    pub fn is_valid(&self) -> bool {
        self.result.converged
    }

    pub fn rank_cmp(left: &Self, right: &Self) -> Ordering {
        match (left.is_valid(), right.is_valid()) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => left
                .result
                .energy
                .partial_cmp(&right.result.energy)
                .unwrap_or(Ordering::Greater),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScottGaPendingRepopulation {
    pub slot_index: usize,
    pub candidate: Candidate,
    pub origin: ScottGaOrigin,
    pub lineage: WorkflowLineage,
}

#[derive(Debug, Clone)]
pub struct ScottGaPopulationTransition {
    pub boundary_population: Vec<ScottGaMember>,
    pub working_population: Vec<ScottGaMember>,
}

/// Duplicate policy seam for Rust-owned GA parity work.
pub trait DuplicatePolicy {
    fn classify(&self, left: &ScottGaMember, right: &ScottGaMember)
        -> Option<ScottDuplicateReason>;
}

/// Duplicate reasons aligned with the native SCOTT duplicate ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScottDuplicateReason {
    Hashkey,
    Pmoi,
    EnergyTol,
}

impl ScottDuplicateReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hashkey => "HASHKEY",
            Self::Pmoi => "PMOI",
            Self::EnergyTol => "ENERGY_TOL",
        }
    }
}

/// Fallback duplicate classifier for SCOTT-shaped Rust GA work.
///
/// Exact native hashkey/topology identity is not computed here. An outer adapter
/// may supply canonical native hashkey evidence first, then delegate to this
/// classifier for the remaining `PMOI -> energy tolerance` ordering.
#[derive(Debug, Clone)]
pub struct ScottDuplicateClassifier {
    pub topology_cutoff: f64,
    pub energy_tolerance: f64,
    pub pmoi_tolerance: f64,
    pub enable_pmoi: bool,
}

impl Default for ScottDuplicateClassifier {
    fn default() -> Self {
        Self {
            topology_cutoff: 2.6,
            energy_tolerance: 0.01,
            pmoi_tolerance: 0.1,
            enable_pmoi: false,
        }
    }
}

impl ScottDuplicateClassifier {
    pub fn pmoi_energy_fallback() -> Self {
        Self {
            enable_pmoi: true,
            ..Self::default()
        }
    }

    pub fn pmoi_energy_fallback_with_pmoi_tolerance(pmoi_tolerance: f64) -> Self {
        Self {
            pmoi_tolerance,
            ..Self::pmoi_energy_fallback()
        }
    }

    pub fn classify_after_hashkey_probe(
        &self,
        exact_hashkey_match: bool,
        left: &ScottGaMember,
        right: &ScottGaMember,
    ) -> Option<ScottDuplicateReason> {
        if exact_hashkey_match {
            return Some(ScottDuplicateReason::Hashkey);
        }

        self.classify(left, right)
    }
}

impl DuplicatePolicy for ScottDuplicateClassifier {
    fn classify(
        &self,
        left: &ScottGaMember,
        right: &ScottGaMember,
    ) -> Option<ScottDuplicateReason> {
        if !left.is_valid() || !right.is_valid() {
            return None;
        }

        if left.result.relaxed_candidate.declared_dimensionality()
            != right.result.relaxed_candidate.declared_dimensionality()
        {
            return None;
        }

        if left.result.relaxed_candidate.len() != right.result.relaxed_candidate.len() {
            return None;
        }

        if self.enable_pmoi
            && left.result.relaxed_candidate.is_zero_d()
            && right.result.relaxed_candidate.is_zero_d()
            && compare_pmoi_scott_intent(
                &left.result.relaxed_candidate,
                &right.result.relaxed_candidate,
                self.pmoi_tolerance,
            )
        {
            return Some(ScottDuplicateReason::Pmoi);
        }

        if left.is_valid()
            && right.is_valid()
            && (left.result.energy - right.result.energy).abs() < self.energy_tolerance
        {
            return Some(ScottDuplicateReason::EnergyTol);
        }

        None
    }
}

/// Per-generation trace emitted by the parity controller.
#[derive(Debug, Clone)]
pub struct ScottGaGenerationTrace {
    pub generation: usize,
    pub stage: ScottGaStage,
    pub selected_indices: Vec<usize>,
    pub population_size: usize,
    pub valid_population_size: usize,
    pub child_count: usize,
    pub duplicate_count: usize,
    pub duplicate_hashkey_count: usize,
    pub duplicate_pmoi_count: usize,
    pub duplicate_energy_tol_count: usize,
    pub repopulated_count: usize,
    pub best_energy: Option<f64>,
}

#[derive(Debug, Clone, Copy)]
pub struct ScottGaOperatorConfig {
    pub pop_replacement_ratio: f64,
    pub reinsert_elites_ratio: f64,
    pub max_repop_attempts: usize,
    pub mutation_ratio: f64,
    pub mut_selfcross_ratio: f64,
    pub cross_1d_2d_ratio: f64,
    pub dim_tolerance: f64,
    pub mutate_swap_ratio: f64,
    pub mutate_expand_ratio: f64,
    pub mutate_contract_ratio: f64,
    pub cluster_crossover_fragment_offset_z: f64,
    pub crossover_attempts: usize,
    pub tournament_size_min: usize,
    pub tournament_size_max: usize,
}

impl Default for ScottGaOperatorConfig {
    fn default() -> Self {
        Self {
            pop_replacement_ratio: 0.95,
            reinsert_elites_ratio: 0.5,
            max_repop_attempts: 10_000,
            mutation_ratio: 0.8,
            mut_selfcross_ratio: 0.2,
            cross_1d_2d_ratio: 1.0,
            dim_tolerance: 0.0,
            mutate_swap_ratio: 0.35,
            mutate_expand_ratio: 0.15,
            mutate_contract_ratio: 0.15,
            cluster_crossover_fragment_offset_z: 1.025,
            crossover_attempts: 4,
            tournament_size_min: 2,
            tournament_size_max: 5,
        }
    }
}

fn build_duplicate_member(candidate: &Candidate, energy: f64, converged: bool) -> ScottGaMember {
    let mut relaxed_candidate = candidate.clone();
    if relaxed_candidate.is_zero_d() {
        super::standardize_candidate_coordinates(&mut relaxed_candidate);
    }

    ScottGaMember::new(
        candidate.clone(),
        EvalResult {
            energy,
            forces: vec![[0.0, 0.0, 0.0]; relaxed_candidate.len()],
            relaxed_candidate,
            converged,
            wall_time: std::time::Duration::from_secs(0),
        },
        ScottGaOrigin::Seed,
        WorkflowLineage::seed(candidate.label.clone()),
    )
}

pub fn classify_duplicate_candidates_after_hashkey_probe(
    exact_hashkey_match: bool,
    left: DuplicateCandidateSnapshot<'_>,
    right: DuplicateCandidateSnapshot<'_>,
    classifier: &ScottDuplicateClassifier,
) -> Option<ScottDuplicateReason> {
    let left_member = build_duplicate_member(left.candidate, left.energy, left.converged);
    let right_member = build_duplicate_member(right.candidate, right.energy, right.converged);
    classifier.classify_after_hashkey_probe(exact_hashkey_match, &left_member, &right_member)
}

pub fn classify_duplicate_candidates(
    left: DuplicateCandidateSnapshot<'_>,
    right: DuplicateCandidateSnapshot<'_>,
    classifier: &ScottDuplicateClassifier,
) -> Option<ScottDuplicateReason> {
    classify_duplicate_candidates_after_hashkey_probe(false, left, right, classifier)
}

#[derive(Debug, Clone, Copy)]
pub struct DuplicateCandidateSnapshot<'a> {
    pub candidate: &'a Candidate,
    pub energy: f64,
    pub converged: bool,
}

#[derive(Debug, Clone)]
pub struct ScottParityGeneticAlgorithm<D = ScottDuplicateClassifier> {
    pub(crate) base: Candidate,
    pub(crate) geometry_model: super::ScottGeometryModel,
    pub(crate) rng: super::TinyRng,
    pub(crate) cfg: Option<SearchConfig>,
    pub(crate) operator_cfg: ScottGaOperatorConfig,
    pub(crate) duplicate_policy: D,
    pub(crate) trace: Vec<ScottGaGenerationTrace>,
}
