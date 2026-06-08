use anyhow::{anyhow, Result};
use patina_emulate::{
    AcquisitionRecord, BranchId, CampaignId, CandidateObservation, FeatureProjectionPort,
    FidelityClass, LearningObjective, SurrogateConfig, SurrogateScoringPort, TaskDirection,
};
use patina_runtime::ScottBackendRoutingPolicy;
use patina_search::FixedParentHybridCrossoverConfig;
use patina_types::{
    HybridCrossoverAttemptRecord, HybridCrossoverConfig, HybridCrossoverScientificMode,
    RestartSeedProvenance,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use super::ga_emulate::{
    build_ga_training_observations_from_execution, GaEmulateScoringRequest,
    GaEmulateScoringService, IdentifiedCandidateInput,
};
use super::ga_execution::RustJanusGaExecution;
use super::ports::{
    HybridGaProductionArtifactSink, HybridGaStagePort, HybridProductionStagePort,
    HybridSeedSelectionPort,
};
use super::scott_production::{
    ProductionRestartState, ProductionRunConfig, ProductionSeedInput, ProductionWorkflowExecution,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HybridSeedSelectionMode {
    TopRanked,
    AcquisitionGuided,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HybridSeedCandidateMode {
    RelaxedResult,
    SourceCandidate,
}

#[derive(Debug, Clone)]
pub struct HybridGaProductionRequest {
    pub system: String,
    pub workdir: PathBuf,
    pub max_production_seeds: usize,
    pub seed_selection_mode: HybridSeedSelectionMode,
    pub seed_candidate_mode: HybridSeedCandidateMode,
    pub crossover_config: HybridCrossoverConfig,
    pub emulate_context: Option<HybridEmulateContext>,
    pub require_converged_ga_seeds: bool,
    pub seed: u64,
    pub production_config: ProductionRunConfig,
    pub restart_state_before: Option<ProductionRestartState>,
    pub restart_counter_base: usize,
    pub fallback_routing_policy: ScottBackendRoutingPolicy,
}

impl HybridGaProductionRequest {
    pub fn validate(&self) -> Result<()> {
        if self.system.trim().is_empty() {
            return Err(anyhow!("hybrid workflow system must not be empty"));
        }
        if self.workdir.as_os_str().is_empty() {
            return Err(anyhow!("hybrid workflow workdir must not be empty"));
        }
        if self.max_production_seeds == 0 {
            return Err(anyhow!(
                "hybrid workflow max_production_seeds must be at least one"
            ));
        }
        if matches!(
            self.crossover_config.scientific_mode,
            HybridCrossoverScientificMode::EnforcedCrossover
        ) && self.crossover_config.attempt_count == 0
        {
            return Err(anyhow!(
                "hybrid workflow enforced crossover requires at least one crossover attempt"
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct HybridEmulateContext {
    pub campaign_id: CampaignId,
    pub branch_id: BranchId,
    pub fidelity: FidelityClass,
}

#[derive(Debug, Clone)]
pub struct HybridGaStageRequest {
    pub system: String,
    pub workdir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct HybridProductionStageRequest {
    pub system: String,
    pub workdir: PathBuf,
    pub seeds: Vec<ProductionSeedInput>,
    pub production_config: ProductionRunConfig,
    pub restart_state_before: Option<ProductionRestartState>,
    pub restart_counter_base: usize,
    pub fallback_routing_policy: ScottBackendRoutingPolicy,
}

#[derive(Debug, Clone)]
pub struct HybridSeedSelectionRequest {
    pub system: String,
    pub max_production_seeds: usize,
    pub seed_selection_mode: HybridSeedSelectionMode,
    pub require_converged_ga_seeds: bool,
    pub training_observations: Option<Vec<CandidateObservation>>,
    pub candidates: Vec<HybridSeedSelectionCandidate>,
}

#[derive(Debug, Clone)]
pub struct HybridSeedSelectionCandidate {
    pub candidate_id: String,
    pub ga_population_index: usize,
    pub origin: String,
    pub converged: bool,
    pub source_candidate: patina_types::Candidate,
    pub selected_candidate: patina_types::Candidate,
    pub selected_energy: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HybridSeedRankingBasis {
    TopRankedEnergy,
    AcquisitionGuided,
}

#[derive(Debug, Clone, Serialize)]
pub struct HybridSelectionGuidance {
    pub provenance_label: String,
    pub direction: TaskDirection,
    pub objective: LearningObjective,
    pub acquisition_score: f64,
    pub uncertainty_score: f64,
    pub expected_improvement: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct HybridSeedSelectionDecision {
    pub candidate_id: String,
    pub selection_rank: usize,
    pub ranking_basis: HybridSeedRankingBasis,
    pub guidance: Option<HybridSelectionGuidance>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HybridSelectedSeed {
    pub candidate_id: String,
    pub ga_rank: usize,
    pub selection_rank: usize,
    pub crossover_attempt: HybridCrossoverAttemptRecord,
    pub source_name: String,
    pub seed_provenance: RestartSeedProvenance,
    pub origin: String,
    pub converged: bool,
    pub source_candidate_label: String,
    pub selected_candidate_label: String,
    pub selected_energy: f64,
    pub selection_mode: HybridSeedSelectionMode,
    pub candidate_mode: HybridSeedCandidateMode,
    pub ranking_basis: HybridSeedRankingBasis,
    pub guidance: Option<HybridSelectionGuidance>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HybridAcceptedChild {
    pub candidate_id: String,
    pub production_child_rank: usize,
    pub crossover_attempt: HybridCrossoverAttemptRecord,
    pub source_name: String,
    pub seed_provenance: RestartSeedProvenance,
    pub child_candidate_label: String,
    pub parent_ga_ranks: Vec<usize>,
    pub parent_source_candidate_labels: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HybridGaProductionSummary {
    pub system: String,
    pub ga_population_size: usize,
    pub scientific_mode: HybridCrossoverScientificMode,
    pub selected_seed_count: usize,
    pub accepted_child_count: usize,
    pub production_input_count: usize,
    pub require_converged_ga_seeds: bool,
    pub seed_selection_mode: HybridSeedSelectionMode,
    pub seed_candidate_mode: HybridSeedCandidateMode,
    pub production_success_count: usize,
    pub production_failure_count: usize,
    pub production_topology_skip_count: usize,
    pub best_set_size: usize,
}

#[derive(Debug, Clone)]
pub struct HybridGaProductionExecution {
    pub summary: HybridGaProductionSummary,
    pub selected_seeds: Vec<HybridSelectedSeed>,
    pub accepted_children: Vec<HybridAcceptedChild>,
    pub ga_execution: RustJanusGaExecution,
    pub production_execution: ProductionWorkflowExecution,
}

#[derive(Debug, Clone)]
struct SelectedHybridCandidate {
    seed: HybridSelectedSeed,
    candidate: HybridSeedSelectionCandidate,
    production_seed: ProductionSeedInput,
}

pub struct HybridGaProductionWorkflowService;

impl HybridGaProductionWorkflowService {
    pub fn execute(
        &self,
        request: &HybridGaProductionRequest,
        ga_stage_port: &dyn HybridGaStagePort,
        production_stage_port: &dyn HybridProductionStagePort,
        seed_selection_port: &dyn HybridSeedSelectionPort,
        artifact_sink: &dyn HybridGaProductionArtifactSink,
    ) -> Result<HybridGaProductionExecution> {
        let execution = execute_hybrid_ga_production_workflow(
            request,
            ga_stage_port,
            production_stage_port,
            seed_selection_port,
        )?;
        artifact_sink.persist_hybrid_ga_production_run(&execution)?;
        Ok(execution)
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct TopRankedHybridSeedSelectionPolicy;

impl HybridSeedSelectionPort for TopRankedHybridSeedSelectionPolicy {
    fn select_seeds(
        &self,
        request: &HybridSeedSelectionRequest,
    ) -> Result<Vec<HybridSeedSelectionDecision>> {
        if request.system.trim().is_empty() {
            return Err(anyhow!(
                "hybrid seed selection request system must not be empty"
            ));
        }
        if request.seed_selection_mode != HybridSeedSelectionMode::TopRanked {
            return Err(anyhow!(
                "top-ranked hybrid seed selection policy cannot service mode `{:?}`",
                request.seed_selection_mode
            ));
        }

        let mut eligible = request
            .candidates
            .iter()
            .filter(|candidate| !request.require_converged_ga_seeds || candidate.converged)
            .collect::<Vec<_>>();
        eligible.sort_by(|left, right| hybrid_selection_candidate_cmp(left, right));

        Ok(eligible
            .into_iter()
            .take(request.max_production_seeds)
            .enumerate()
            .map(|(index, candidate)| HybridSeedSelectionDecision {
                candidate_id: candidate.candidate_id.clone(),
                selection_rank: index + 1,
                ranking_basis: HybridSeedRankingBasis::TopRankedEnergy,
                guidance: None,
            })
            .collect())
    }
}

#[derive(Debug, Clone)]
pub struct StaticAcquisitionGuidedHybridSeedSelectionPolicy {
    provenance_label: String,
    acquisitions_by_candidate_id: BTreeMap<String, AcquisitionRecord>,
}

impl StaticAcquisitionGuidedHybridSeedSelectionPolicy {
    pub fn new(
        provenance_label: impl Into<String>,
        acquisitions: Vec<AcquisitionRecord>,
    ) -> Result<Self> {
        let mut acquisitions_by_candidate_id = BTreeMap::new();
        for acquisition in acquisitions {
            let candidate_id = acquisition.candidate_id.clone();
            if acquisitions_by_candidate_id
                .insert(candidate_id.clone(), acquisition)
                .is_some()
            {
                return Err(anyhow!(
                    "duplicate acquisition guidance for hybrid candidate `{candidate_id}`"
                ));
            }
        }
        Ok(Self {
            provenance_label: provenance_label.into(),
            acquisitions_by_candidate_id,
        })
    }
}

impl HybridSeedSelectionPort for StaticAcquisitionGuidedHybridSeedSelectionPolicy {
    fn select_seeds(
        &self,
        request: &HybridSeedSelectionRequest,
    ) -> Result<Vec<HybridSeedSelectionDecision>> {
        if request.system.trim().is_empty() {
            return Err(anyhow!(
                "hybrid seed selection request system must not be empty"
            ));
        }
        if request.seed_selection_mode != HybridSeedSelectionMode::AcquisitionGuided {
            return Err(anyhow!(
                "acquisition-guided hybrid seed selection policy cannot service mode `{:?}`",
                request.seed_selection_mode
            ));
        }

        let eligible = request
            .candidates
            .iter()
            .filter(|candidate| !request.require_converged_ga_seeds || candidate.converged)
            .collect::<Vec<_>>();
        if eligible.is_empty() {
            return Ok(Vec::new());
        }

        let mut guided = eligible
            .into_iter()
            .map(|candidate| {
                let acquisition = self
                    .acquisitions_by_candidate_id
                    .get(&candidate.candidate_id)
                    .ok_or_else(|| {
                        anyhow!(
                            "acquisition-guided hybrid seed selection requires guidance for candidate `{}`",
                            candidate.candidate_id
                        )
                    })?;
                Ok((candidate, acquisition))
            })
            .collect::<Result<Vec<_>>>()?;

        guided.sort_by(
            |(left_candidate, left_acquisition), (right_candidate, right_acquisition)| {
                left_acquisition
                    .rank
                    .cmp(&right_acquisition.rank)
                    .then_with(|| {
                        right_acquisition
                            .acquisition_score
                            .partial_cmp(&left_acquisition.acquisition_score)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .then_with(|| {
                        right_acquisition
                            .uncertainty_score
                            .partial_cmp(&left_acquisition.uncertainty_score)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .then_with(|| hybrid_selection_candidate_cmp(left_candidate, right_candidate))
            },
        );

        Ok(guided
            .into_iter()
            .take(request.max_production_seeds)
            .enumerate()
            .map(
                |(index, (candidate, acquisition))| HybridSeedSelectionDecision {
                    candidate_id: candidate.candidate_id.clone(),
                    selection_rank: index + 1,
                    ranking_basis: HybridSeedRankingBasis::AcquisitionGuided,
                    guidance: Some(HybridSelectionGuidance {
                        provenance_label: self.provenance_label.clone(),
                        direction: acquisition.direction,
                        objective: acquisition.objective,
                        acquisition_score: acquisition.acquisition_score,
                        uncertainty_score: acquisition.uncertainty_score,
                        expected_improvement: acquisition.expected_improvement,
                    }),
                },
            )
            .collect())
    }
}

#[derive(Debug, Clone)]
pub struct EmulateBackedHybridSeedSelectionConfig {
    pub objective: LearningObjective,
    pub surrogate: SurrogateConfig,
    pub target_name: String,
    pub target_unit: Option<String>,
    pub provenance_label: Option<String>,
}

pub struct EmulateBackedHybridSeedSelectionAdapter<'a> {
    config: EmulateBackedHybridSeedSelectionConfig,
    feature_projection: &'a dyn FeatureProjectionPort,
    surrogate: &'a dyn SurrogateScoringPort,
    scoring_service: GaEmulateScoringService,
}

impl<'a> EmulateBackedHybridSeedSelectionAdapter<'a> {
    pub fn new(
        config: EmulateBackedHybridSeedSelectionConfig,
        feature_projection: &'a dyn FeatureProjectionPort,
        surrogate: &'a dyn SurrogateScoringPort,
    ) -> Self {
        Self {
            config,
            feature_projection,
            surrogate,
            scoring_service: GaEmulateScoringService,
        }
    }
}

impl HybridSeedSelectionPort for EmulateBackedHybridSeedSelectionAdapter<'_> {
    fn select_seeds(
        &self,
        request: &HybridSeedSelectionRequest,
    ) -> Result<Vec<HybridSeedSelectionDecision>> {
        if request.system.trim().is_empty() {
            return Err(anyhow!(
                "hybrid seed selection request system must not be empty"
            ));
        }
        if request.seed_selection_mode != HybridSeedSelectionMode::AcquisitionGuided {
            return Err(anyhow!(
                "emulate-backed hybrid seed selection adapter requires `acquisition_guided` mode"
            ));
        }
        let training_observations = request.training_observations.clone().ok_or_else(|| {
            anyhow!(
                "emulate-backed hybrid seed selection requires training observations in the selection request"
            )
        })?;

        let identified_candidates = request
            .candidates
            .iter()
            .filter(|candidate| !request.require_converged_ga_seeds || candidate.converged)
            .map(|candidate| IdentifiedCandidateInput {
                candidate_id: candidate.candidate_id.clone(),
                candidate: candidate.selected_candidate.clone(),
                metadata: [
                    ("label".into(), candidate.selected_candidate.label.clone()),
                    ("origin".into(), candidate.origin.clone()),
                    (
                        "ga_population_index".into(),
                        candidate.ga_population_index.to_string(),
                    ),
                ]
                .into_iter()
                .collect(),
            })
            .collect::<Vec<_>>();

        if identified_candidates.is_empty() {
            return Ok(Vec::new());
        }

        let emulate_context = training_observations
            .first()
            .ok_or_else(|| anyhow!("hybrid emulate selection requires at least one observation"))?;
        let scoring_execution =
            self.scoring_service
                .score_identified_candidates_from_observations(
                    &GaEmulateScoringRequest {
                        campaign_id: emulate_context.campaign_id.clone(),
                        branch_id: emulate_context.branch_id.clone(),
                        objective: self.config.objective,
                        fidelity: emulate_context.provenance.fidelity.ok_or_else(|| {
                            anyhow!("hybrid emulate observation is missing fidelity")
                        })?,
                        surrogate: self.config.surrogate.clone(),
                        target_name: self.config.target_name.clone(),
                        target_unit: self.config.target_unit.clone(),
                        provenance_label: self.config.provenance_label.clone(),
                    },
                    training_observations,
                    &identified_candidates,
                    self.feature_projection,
                    self.surrogate,
                )?;
        let acquisition_policy = StaticAcquisitionGuidedHybridSeedSelectionPolicy::new(
            self.config
                .provenance_label
                .clone()
                .unwrap_or_else(|| "patina_driver.hybrid_emulate_selection".into()),
            scoring_execution.acquisitions,
        )?;
        acquisition_policy.select_seeds(request)
    }
}

fn execute_hybrid_ga_production_workflow(
    request: &HybridGaProductionRequest,
    ga_stage_port: &dyn HybridGaStagePort,
    production_stage_port: &dyn HybridProductionStagePort,
    seed_selection_port: &dyn HybridSeedSelectionPort,
) -> Result<HybridGaProductionExecution> {
    request.validate()?;
    let ga_execution = ga_stage_port.execute_ga_stage(&HybridGaStageRequest {
        system: request.system.clone(),
        workdir: request.workdir.join("ga"),
    })?;
    let (selected_seeds, accepted_children, production_seeds) =
        select_production_seeds(request, &ga_execution, seed_selection_port)?;
    let production_execution =
        production_stage_port.execute_production_stage(&HybridProductionStageRequest {
            system: request.system.clone(),
            workdir: request.workdir.join("production"),
            seeds: production_seeds,
            production_config: request.production_config.clone(),
            restart_state_before: request.restart_state_before.clone(),
            restart_counter_base: request.restart_counter_base,
            fallback_routing_policy: request.fallback_routing_policy.clone(),
        })?;

    let summary = HybridGaProductionSummary {
        system: request.system.clone(),
        ga_population_size: ga_execution.final_population.len(),
        scientific_mode: request.crossover_config.scientific_mode,
        selected_seed_count: selected_seeds.len(),
        accepted_child_count: accepted_children.len(),
        production_input_count: production_execution.summary.candidate_count,
        require_converged_ga_seeds: request.require_converged_ga_seeds,
        seed_selection_mode: request.seed_selection_mode,
        seed_candidate_mode: request.seed_candidate_mode,
        production_success_count: production_execution.summary.success_count,
        production_failure_count: production_execution.summary.failure_count,
        production_topology_skip_count: production_execution.summary.topology_skip_count,
        best_set_size: production_execution.summary.best_set_size,
    };

    Ok(HybridGaProductionExecution {
        summary,
        selected_seeds,
        accepted_children,
        ga_execution,
        production_execution,
    })
}

fn select_production_seeds(
    request: &HybridGaProductionRequest,
    ga_execution: &RustJanusGaExecution,
    seed_selection_port: &dyn HybridSeedSelectionPort,
) -> Result<(
    Vec<HybridSelectedSeed>,
    Vec<HybridAcceptedChild>,
    Vec<ProductionSeedInput>,
)> {
    let selection_request = build_seed_selection_request(request, ga_execution)?;
    let ga_generation = ga_execution
        .generation_artifacts
        .last()
        .map(|artifact| artifact.generation);
    let mut selections = seed_selection_port.select_seeds(&selection_request)?;
    if selections.is_empty() {
        return Err(anyhow!(
            "hybrid workflow found no GA members eligible for production seeding"
        ));
    }
    selections.sort_by_key(|decision| decision.selection_rank);

    let ga_rank_by_candidate_id = ga_rank_by_candidate_id(&selection_request.candidates);
    let mut candidates_by_id = selection_request
        .candidates
        .into_iter()
        .map(|candidate| (candidate.candidate_id.clone(), candidate))
        .collect::<BTreeMap<_, _>>();
    let mut seen_candidate_ids = BTreeSet::new();
    let mut selected_candidates = Vec::with_capacity(selections.len());

    for decision in selections {
        if !seen_candidate_ids.insert(decision.candidate_id.clone()) {
            return Err(anyhow!(
                "hybrid seed selection returned duplicate candidate `{}`",
                decision.candidate_id
            ));
        }
        let candidate = candidates_by_id
            .remove(&decision.candidate_id)
            .ok_or_else(|| {
                anyhow!(
                    "hybrid seed selection referenced unknown candidate `{}`",
                    decision.candidate_id
                )
            })?;
        let ga_rank = *ga_rank_by_candidate_id
            .get(&decision.candidate_id)
            .ok_or_else(|| anyhow!("missing GA rank for candidate `{}`", decision.candidate_id))?;
        let source_name = format!(
            "ga_rank_{:04}_{}_{}",
            ga_rank,
            candidate.origin,
            sanitize_seed_label(&candidate.selected_candidate.label)
        );
        let seed_input = ProductionSeedInput::hybrid_selection(
            source_name.clone(),
            candidate.source_candidate.label.clone(),
            candidate.selected_candidate.clone(),
        );
        let selected = selected_seed(
            ga_rank,
            &decision,
            &candidate,
            request,
            ga_generation,
            source_name,
            seed_input.provenance.clone(),
        );
        selected_candidates.push(SelectedHybridCandidate {
            seed: selected,
            candidate,
            production_seed: seed_input,
        });
    }

    let selected_seeds = selected_candidates
        .iter()
        .map(|selected| selected.seed.clone())
        .collect::<Vec<_>>();
    match request.crossover_config.scientific_mode {
        HybridCrossoverScientificMode::GaDownstreamSelection => Ok((
            selected_seeds,
            Vec::new(),
            selected_candidates
                .into_iter()
                .map(|selected| selected.production_seed)
                .collect(),
        )),
        HybridCrossoverScientificMode::EnforcedCrossover => {
            if selected_candidates.len() < 2 {
                return Err(anyhow!(
                    "hybrid enforced crossover requires two selected GA parents, found {}",
                    selected_candidates.len()
                ));
            }
            let left_parent = &selected_candidates[0];
            let right_parent = &selected_candidates[1];
            let (accepted_children, production_seeds) = build_enforced_crossover_children(
                request,
                ga_generation,
                left_parent,
                right_parent,
            )?;
            Ok((selected_seeds, accepted_children, production_seeds))
        }
    }
}

fn build_enforced_crossover_children(
    request: &HybridGaProductionRequest,
    ga_generation: Option<usize>,
    left_parent: &SelectedHybridCandidate,
    right_parent: &SelectedHybridCandidate,
) -> Result<(Vec<HybridAcceptedChild>, Vec<ProductionSeedInput>)> {
    let accepted = patina_search::run_fixed_parent_hybrid_crossover(
        &left_parent.candidate.selected_candidate,
        &right_parent.candidate.selected_candidate,
        FixedParentHybridCrossoverConfig {
            ga_generation,
            attempt_count: request.crossover_config.attempt_count,
            target_children: request.max_production_seeds,
            seed: request.seed,
        },
    );
    if accepted.is_empty() {
        return Err(anyhow!(
            "hybrid enforced crossover did not produce any accepted children"
        ));
    }

    let parent_source_label = format!(
        "{}__{}",
        left_parent.candidate.source_candidate.label.as_str(),
        right_parent.candidate.source_candidate.label.as_str()
    );
    let parent_ga_ranks = vec![left_parent.seed.ga_rank, right_parent.seed.ga_rank];
    let parent_source_candidate_labels = vec![
        left_parent.candidate.source_candidate.label.clone(),
        right_parent.candidate.source_candidate.label.clone(),
    ];

    let mut accepted_children = Vec::with_capacity(accepted.len());
    let mut production_seeds = Vec::with_capacity(accepted.len());
    for (index, child) in accepted.into_iter().enumerate() {
        let production_child_rank = index + 1;
        let attempt_index = child.attempt.attempt_index.unwrap_or(0);
        let source_name = format!(
            "hybrid_child_{:04}_ga_{:04}_{:04}_attempt_{:04}_{}",
            production_child_rank,
            left_parent.seed.ga_rank,
            right_parent.seed.ga_rank,
            attempt_index,
            sanitize_seed_label(&child.candidate.label)
        );
        let seed_input = ProductionSeedInput::hybrid_crossover_child(
            source_name.clone(),
            parent_source_label.clone(),
            child.candidate.clone(),
        );
        accepted_children.push(HybridAcceptedChild {
            candidate_id: child.candidate.label.clone(),
            production_child_rank,
            crossover_attempt: child.attempt,
            source_name,
            seed_provenance: seed_input.provenance.clone(),
            child_candidate_label: child.candidate.label,
            parent_ga_ranks: parent_ga_ranks.clone(),
            parent_source_candidate_labels: parent_source_candidate_labels.clone(),
        });
        production_seeds.push(seed_input);
    }

    Ok((accepted_children, production_seeds))
}

fn selected_seed(
    ga_rank: usize,
    decision: &HybridSeedSelectionDecision,
    candidate: &HybridSeedSelectionCandidate,
    request: &HybridGaProductionRequest,
    ga_generation: Option<usize>,
    source_name: String,
    seed_provenance: RestartSeedProvenance,
) -> HybridSelectedSeed {
    HybridSelectedSeed {
        candidate_id: candidate.candidate_id.clone(),
        ga_rank,
        selection_rank: decision.selection_rank,
        crossover_attempt: patina_search::build_hybrid_crossover_attempt_record(
            patina_search::HybridCrossoverAttemptInput {
                scientific_mode: HybridCrossoverScientificMode::GaDownstreamSelection,
                ga_generation,
                attempt_index: None,
                child_origin_label: &candidate.origin,
                selected_for_production: true,
                parent_left_label: None,
                parent_right_label: None,
            },
        ),
        source_name,
        seed_provenance,
        origin: candidate.origin.clone(),
        converged: candidate.converged,
        source_candidate_label: candidate.source_candidate.label.clone(),
        selected_candidate_label: candidate.selected_candidate.label.clone(),
        selected_energy: candidate.selected_energy,
        selection_mode: request.seed_selection_mode,
        candidate_mode: request.seed_candidate_mode,
        ranking_basis: decision.ranking_basis,
        guidance: decision.guidance.clone(),
    }
}

fn build_seed_selection_request(
    request: &HybridGaProductionRequest,
    ga_execution: &RustJanusGaExecution,
) -> Result<HybridSeedSelectionRequest> {
    Ok(HybridSeedSelectionRequest {
        system: request.system.clone(),
        max_production_seeds: match request.crossover_config.scientific_mode {
            HybridCrossoverScientificMode::GaDownstreamSelection => request.max_production_seeds,
            HybridCrossoverScientificMode::EnforcedCrossover => 2,
        },
        seed_selection_mode: request.seed_selection_mode,
        require_converged_ga_seeds: request.require_converged_ga_seeds,
        training_observations: request
            .emulate_context
            .as_ref()
            .map(|context| {
                build_ga_training_observations_from_execution(
                    &context.campaign_id,
                    &context.branch_id,
                    context.fidelity,
                    ga_execution,
                )
            })
            .transpose()?,
        candidates: ga_execution
            .final_population
            .iter()
            .enumerate()
            .map(|(population_index, member)| {
                let selected_candidate = match request.seed_candidate_mode {
                    HybridSeedCandidateMode::RelaxedResult => {
                        member.result.relaxed_candidate.clone()
                    }
                    HybridSeedCandidateMode::SourceCandidate => member.source_candidate.clone(),
                };
                HybridSeedSelectionCandidate {
                    candidate_id: build_hybrid_seed_candidate_id(
                        population_index,
                        member.origin.as_str(),
                        &selected_candidate.label,
                    ),
                    ga_population_index: population_index,
                    origin: member.origin.as_str().to_string(),
                    converged: member.result.converged,
                    source_candidate: member.source_candidate.clone(),
                    selected_candidate,
                    selected_energy: member.result.energy,
                }
            })
            .collect(),
    })
}

fn ga_rank_by_candidate_id(candidates: &[HybridSeedSelectionCandidate]) -> BTreeMap<String, usize> {
    let mut ranked = candidates.iter().collect::<Vec<_>>();
    ranked.sort_by(|left, right| hybrid_selection_candidate_cmp(left, right));
    ranked
        .into_iter()
        .enumerate()
        .map(|(index, candidate)| (candidate.candidate_id.clone(), index + 1))
        .collect()
}

pub fn build_hybrid_seed_candidate_id(
    population_index: usize,
    origin: &str,
    selected_label: &str,
) -> String {
    format!(
        "hybrid_seed_{population_index:04}_{origin}_{}",
        sanitize_seed_label(selected_label)
    )
}

fn hybrid_selection_candidate_cmp(
    left: &HybridSeedSelectionCandidate,
    right: &HybridSeedSelectionCandidate,
) -> std::cmp::Ordering {
    match (left.converged, right.converged) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => left
            .selected_energy
            .partial_cmp(&right.selected_energy)
            .unwrap_or(std::cmp::Ordering::Greater)
            .then_with(|| {
                left.selected_candidate
                    .label
                    .cmp(&right.selected_candidate.label)
            })
            .then_with(|| left.ga_population_index.cmp(&right.ga_population_index)),
    }
}

fn sanitize_seed_label(label: &str) -> String {
    label
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-' => ch,
            _ => '_',
        })
        .collect()
}
