use anyhow::{anyhow, bail, Context, Result};
use patina_emulate::{
    AcquisitionRecord, BranchFamily, BranchId, CandidateObservation, FeatureProjectionPort,
    FeatureRepresentation, FidelityClass, LearningObjective, ObservationOutcome,
    ObservationProvenance, PredictionHeadKind, PredictionTarget, SurrogateScoringPort,
    SurrogateScoringRequest, SurrogateScoringResponse, SurrogateTask, TaskDirection,
};
use patina_types::{Candidate, EvaluationRecord, StructureRecord};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use super::ga_execution::RustJanusGaExecution;

#[derive(Debug, Clone)]
pub struct GaEmulateScoringRequest {
    pub campaign_id: patina_emulate::CampaignId,
    pub branch_id: BranchId,
    pub objective: LearningObjective,
    pub fidelity: FidelityClass,
    pub surrogate: patina_emulate::SurrogateConfig,
    pub target_name: String,
    pub target_unit: Option<String>,
    pub provenance_label: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GaEmulateScoringExecution {
    pub observations: Vec<CandidateObservation>,
    pub feature_representation: FeatureRepresentation,
    pub scoring_request: SurrogateScoringRequest,
    pub scoring_response: SurrogateScoringResponse,
    pub acquisitions: Vec<AcquisitionRecord>,
}

#[derive(Debug, Clone)]
pub struct IdentifiedCandidateInput {
    pub candidate_id: String,
    pub candidate: Candidate,
    pub metadata: BTreeMap<String, String>,
}

pub struct GaEmulateScoringService;

impl GaEmulateScoringService {
    #[cfg(test)]
    pub fn score_pending_candidates(
        &self,
        request: &GaEmulateScoringRequest,
        execution: &RustJanusGaExecution,
        pending_candidates: &[Candidate],
        feature_projection: &dyn FeatureProjectionPort,
        surrogate: &dyn SurrogateScoringPort,
    ) -> Result<GaEmulateScoringExecution> {
        if pending_candidates.is_empty() {
            bail!("GA emulate scoring requires at least one pending candidate");
        }
        let observations = build_ga_training_observations_from_execution(
            &request.campaign_id,
            &request.branch_id,
            request.fidelity,
            execution,
        )?;
        self.score_pending_candidates_from_observations(
            request,
            observations,
            pending_candidates,
            feature_projection,
            surrogate,
        )
    }

    pub fn score_pending_candidates_from_observations(
        &self,
        request: &GaEmulateScoringRequest,
        observations: Vec<CandidateObservation>,
        pending_candidates: &[Candidate],
        feature_projection: &dyn FeatureProjectionPort,
        surrogate: &dyn SurrogateScoringPort,
    ) -> Result<GaEmulateScoringExecution> {
        let identified_candidates = pending_candidates
            .iter()
            .enumerate()
            .map(|(index, candidate)| IdentifiedCandidateInput {
                candidate_id: format!("ga_pending_{index:04}"),
                candidate: candidate.clone(),
                metadata: [
                    ("label".into(), candidate.label.clone()),
                    ("pending_index".into(), index.to_string()),
                ]
                .into_iter()
                .collect(),
            })
            .collect::<Vec<_>>();
        self.score_identified_candidates_from_observations(
            request,
            observations,
            &identified_candidates,
            feature_projection,
            surrogate,
        )
    }

    pub fn score_identified_candidates_from_observations(
        &self,
        request: &GaEmulateScoringRequest,
        mut observations: Vec<CandidateObservation>,
        pending_candidates: &[IdentifiedCandidateInput],
        feature_projection: &dyn FeatureProjectionPort,
        surrogate: &dyn SurrogateScoringPort,
    ) -> Result<GaEmulateScoringExecution> {
        if observations.len() < 3 {
            bail!(
                "GA emulate scoring requires at least three unique training observations, got {}",
                observations.len()
            );
        }

        let mut structures = observations
            .iter()
            .filter_map(|observation| observation.evaluated_candidate.as_ref())
            .map(|evaluation| evaluation.structure.clone())
            .collect::<Vec<_>>();
        structures.extend(
            pending_candidates
                .iter()
                .map(|candidate| StructureRecord::from(&candidate.candidate))
                .collect::<Vec<_>>(),
        );

        let representation = feature_projection.derive_representation(&structures)?;
        let training_rows = observations
            .iter_mut()
            .enumerate()
            .map(|(index, observation)| {
                let evaluation = observation
                    .evaluated_candidate
                    .as_ref()
                    .ok_or_else(|| anyhow!("training observation is missing evaluation"))?;
                let candidate_id = format!("ga_train_{index:04}");
                observation.feature_representation = Some(representation.clone());
                observation.feature_vector_ref = Some(candidate_id.clone());
                observation.provenance.descriptor_provenance =
                    Some(representation.provenance_label.clone());
                Ok(patina_emulate::SurrogateTrainingRow {
                    candidate_id,
                    features: feature_projection
                        .project_structure(&evaluation.structure, &representation)?,
                    targets: vec![evaluation.energy],
                    fidelity: request.fidelity,
                    metadata: [
                        ("label".into(), evaluation.label.clone()),
                        ("converged".into(), evaluation.converged.to_string()),
                        (
                            "generation".into(),
                            observation.provenance.generation.unwrap_or(0).to_string(),
                        ),
                        (
                            "source_label".into(),
                            observation.provenance.source_label.clone(),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let candidate_rows = pending_candidates
            .iter()
            .map(|candidate| {
                Ok(patina_emulate::SurrogateCandidateRow {
                    candidate_id: candidate.candidate_id.clone(),
                    features: feature_projection.project_structure(
                        &StructureRecord::from(&candidate.candidate),
                        &representation,
                    )?,
                    requested_fidelity: Some(request.fidelity),
                    metadata: candidate.metadata.clone(),
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let scoring_request = SurrogateScoringRequest {
            schema_version: patina_emulate::SURROGATE_REQUEST_SCHEMA_VERSION.into(),
            campaign_id: request.campaign_id.clone(),
            branch_id: request.branch_id.clone(),
            workflow: BranchFamily::GeneticAlgorithm,
            task: SurrogateTask::ScoreCandidates,
            context: patina_emulate::SurrogateRequestContext {
                direction: TaskDirection::Downstream,
                objective: request.objective,
                feature_representation: representation.clone(),
                targets: vec![PredictionTarget {
                    name: request.target_name.clone(),
                    head_kind: PredictionHeadKind::Scalar,
                    fidelity: request.fidelity,
                    unit: request.target_unit.clone(),
                }],
                provenance_label: request.provenance_label.clone(),
            },
            feature_names: representation.feature_names.clone(),
            target_names: vec![request.target_name.clone()],
            training_rows,
            candidate_rows,
            surrogate: request.surrogate.clone(),
        };
        scoring_request.validate_shape()?;

        let scoring_response = surrogate.score_candidates(&scoring_request)?;
        scoring_response.validate_against(&scoring_request)?;
        let acquisitions = scoring_response.to_acquisition_records();

        Ok(GaEmulateScoringExecution {
            observations,
            feature_representation: representation,
            scoring_request,
            scoring_response,
            acquisitions,
        })
    }
}

pub fn build_ga_training_observations_from_execution(
    campaign_id: &patina_emulate::CampaignId,
    branch_id: &BranchId,
    fidelity: FidelityClass,
    execution: &RustJanusGaExecution,
) -> Result<Vec<CandidateObservation>> {
    let mut seen_structures = BTreeSet::new();
    let mut observations = Vec::new();
    for (generation_index, population) in execution.generation_populations.iter().enumerate() {
        let generation = execution
            .generation_artifacts
            .get(generation_index)
            .map(|artifact| artifact.generation)
            .unwrap_or(generation_index);
        for member in population {
            let evaluation = EvaluationRecord::from(&member.result);
            let structure_key = serde_json::to_string(&evaluation.structure)
                .context("failed to serialize GA evaluation structure for dedupe")?;
            if !seen_structures.insert(structure_key) {
                continue;
            }
            observations.push(CandidateObservation {
                campaign_id: campaign_id.clone(),
                branch_id: branch_id.clone(),
                controller_family: BranchFamily::GeneticAlgorithm,
                source_candidate: StructureRecord::from(&member.source_candidate),
                evaluated_candidate: Some(evaluation),
                outcome: ObservationOutcome::Accepted,
                provenance: ObservationProvenance {
                    branch_id: branch_id.clone(),
                    family: BranchFamily::GeneticAlgorithm,
                    source_label: member.source_candidate.label.clone(),
                    step: None,
                    generation: Some(generation),
                    fidelity: Some(fidelity),
                    descriptor_provenance: None,
                },
                feature_representation: None,
                feature_vector_ref: None,
            });
        }
    }
    Ok(observations)
}

pub fn load_ga_training_observations_from_run_dir(
    campaign_id: &patina_emulate::CampaignId,
    branch_id: &BranchId,
    fidelity: FidelityClass,
    run_dir: &Path,
) -> Result<Vec<CandidateObservation>> {
    let raw_dir = run_dir.join("raw");
    let mut generation_state_paths = fs::read_dir(&raw_dir)
        .with_context(|| format!("failed to read GA raw directory `{}`", raw_dir.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| {
                    name.starts_with("generation_")
                        && name.ends_with("_state.json")
                        && !name.contains("_boundary_")
                })
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    generation_state_paths.sort();
    if generation_state_paths.is_empty() {
        bail!(
            "no GA generation state files were found in `{}`",
            raw_dir.display()
        );
    }

    let mut seen_structures = BTreeSet::new();
    let mut observations = Vec::new();
    for path in generation_state_paths {
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read GA generation state `{}`", path.display()))?;
        let state: patina_types::GaGenerationState = serde_json::from_str(&raw)
            .with_context(|| format!("failed to parse GA generation state `{}`", path.display()))?;
        for member in state.population {
            let structure_key =
                serde_json::to_string(&member.evaluation.structure).with_context(|| {
                    format!("failed to serialize structure from `{}`", path.display())
                })?;
            if !seen_structures.insert(structure_key) {
                continue;
            }
            observations.push(CandidateObservation {
                campaign_id: campaign_id.clone(),
                branch_id: branch_id.clone(),
                controller_family: BranchFamily::GeneticAlgorithm,
                source_candidate: member.source,
                evaluated_candidate: Some(member.evaluation),
                outcome: ObservationOutcome::Accepted,
                provenance: ObservationProvenance {
                    branch_id: branch_id.clone(),
                    family: BranchFamily::GeneticAlgorithm,
                    source_label: path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("generation_state")
                        .to_string(),
                    step: None,
                    generation: Some(state.generation),
                    fidelity: Some(fidelity),
                    descriptor_provenance: None,
                },
                feature_representation: None,
                feature_vector_ref: None,
            });
        }
    }
    Ok(observations)
}

#[cfg(test)]
mod tests {
    use super::{
        load_ga_training_observations_from_run_dir, GaEmulateScoringRequest,
        GaEmulateScoringService,
    };
    use patina_emulate::{
        BranchFamily, BranchId, CampaignId, FidelityClass, LearningObjective,
        SimpleStructureStatisticsProjector, SurrogateCandidatePrediction, SurrogateConfig,
        SurrogateModelVariant, SurrogateObjective, SurrogateScoringPort, SurrogateScoringRequest,
        SurrogateScoringResponse, SurrogateTask, TaskDirection,
    };
    use patina_search::{
        PopulationKernelState, ScottGaGenerationTrace, ScottGaMember, ScottGaOrigin,
    };

    fn candidate(label: &str, offset: f64) -> patina_types::Candidate {
        patina_types::Candidate::cluster(
            label,
            vec!["Mg".into(), "O".into()],
            vec![[0.0 + offset, 0.0, 0.0], [0.5 + offset, 0.5, 0.5]],
        )
    }

    fn member(label: &str, offset: f64, energy: f64) -> ScottGaMember {
        ScottGaMember {
            source_candidate: candidate(&format!("source-{label}"), offset),
            result: patina_types::EvalResult {
                energy,
                forces: vec![[0.0, 0.0, 0.0]; 2],
                relaxed_candidate: candidate(label, offset),
                converged: true,
                wall_time: std::time::Duration::from_secs(0),
            },
            origin: ScottGaOrigin::Mutate,
            occurrences: 1,
            lineage: patina_search::WorkflowLineage::seed(format!("source-{label}")),
            topology: patina_search::ScottGaTopologyIdentity::default(),
        }
    }

    struct ScoringStub;

    impl SurrogateScoringPort for ScoringStub {
        fn score_candidates(
            &self,
            request: &SurrogateScoringRequest,
        ) -> anyhow::Result<SurrogateScoringResponse> {
            let mut predictions = request
                .candidate_rows
                .iter()
                .enumerate()
                .map(|(index, row)| SurrogateCandidatePrediction {
                    candidate_id: row.candidate_id.clone(),
                    means: vec![row.features[0]],
                    variances: vec![1.0 / (index + 1) as f64],
                    acquisition_score: (request.candidate_rows.len() - index) as f64,
                    uncertainty_score: index as f64 + 0.25,
                    rank: index + 1,
                })
                .collect::<Vec<_>>();
            predictions.sort_by_key(|prediction| prediction.rank);
            Ok(SurrogateScoringResponse {
                schema_version: patina_emulate::SURROGATE_RESPONSE_SCHEMA_VERSION.into(),
                campaign_id: request.campaign_id.clone(),
                branch_id: request.branch_id.clone(),
                workflow: BranchFamily::GeneticAlgorithm,
                task: SurrogateTask::ScoreCandidates,
                direction: Some(TaskDirection::Downstream),
                objective: Some(request.context.objective),
                feature_names: request.feature_names.clone(),
                target_names: request.target_names.clone(),
                model_variant: request.surrogate.model_variant,
                selected_model_name: "stub-svgp".into(),
                incumbent_target: -3.0,
                checkpoint_path: None,
                predictions,
            })
        }
    }

    fn sample_execution() -> super::RustJanusGaExecution {
        super::RustJanusGaExecution {
            generation_artifacts: vec![
                crate::application::ga_execution::RustJanusGenerationArtifact {
                    generation: 0,
                    phase: "initialize".into(),
                    request_count: 3,
                    success_count: 3,
                    failure_count: 0,
                    failure_kind_counts: std::collections::BTreeMap::new(),
                    converged_count: 3,
                    elapsed_secs: 0.1,
                    best_energy: Some(-3.0),
                    mean_energy: Some(-2.0),
                    worst_energy: Some(-1.0),
                    boundary_population_size: 3,
                    boundary_valid_population_size: 3,
                    population_size: 3,
                    valid_population_size: 3,
                    duplicate_count: 0,
                    duplicate_hashkey_count: 0,
                    duplicate_pmoi_count: 0,
                    duplicate_energy_tol_count: 0,
                    repopulated_count: 0,
                },
                crate::application::ga_execution::RustJanusGenerationArtifact {
                    generation: 1,
                    phase: "evolve".into(),
                    request_count: 3,
                    success_count: 3,
                    failure_count: 0,
                    failure_kind_counts: std::collections::BTreeMap::new(),
                    converged_count: 3,
                    elapsed_secs: 0.2,
                    best_energy: Some(-4.0),
                    mean_energy: Some(-2.5),
                    worst_energy: Some(-1.0),
                    boundary_population_size: 3,
                    boundary_valid_population_size: 3,
                    population_size: 3,
                    valid_population_size: 3,
                    duplicate_count: 0,
                    duplicate_hashkey_count: 0,
                    duplicate_pmoi_count: 0,
                    duplicate_energy_tol_count: 0,
                    repopulated_count: 0,
                },
            ],
            generation_responses: Vec::new(),
            generation_origin_metrics: Vec::new(),
            generation_boundary_populations: Vec::new(),
            generation_populations: vec![
                vec![member("a", 0.0, -1.0), member("b", 0.1, -2.0)],
                vec![member("b", 0.1, -2.0), member("c", 0.2, -3.0)],
            ],
            generation_boundary_kernel_states: Vec::new(),
            generation_kernel_states: vec![
                PopulationKernelState::new(patina_search::WorkflowFamily::GeneticAlgorithm),
                PopulationKernelState::new(patina_search::WorkflowFamily::GeneticAlgorithm),
            ],
            controller_trace: Vec::<ScottGaGenerationTrace>::new(),
            final_population: vec![member("c", 0.2, -3.0)],
        }
    }

    #[test]
    fn scoring_service_builds_ga_surrogate_request_and_rankings() {
        let service = GaEmulateScoringService;
        let execution = sample_execution();
        let projector = SimpleStructureStatisticsProjector;
        let result = service
            .score_pending_candidates(
                &GaEmulateScoringRequest {
                    campaign_id: CampaignId("campaign-1".into()),
                    branch_id: BranchId("branch-ga-1".into()),
                    objective: LearningObjective::ReduceUncertainty,
                    fidelity: FidelityClass::JanusMaceLow,
                    surrogate: SurrogateConfig {
                        model_variant: SurrogateModelVariant::WhitenedSvgp,
                        objective: SurrogateObjective::Minimize,
                        ..SurrogateConfig::default()
                    },
                    target_name: "energy".into(),
                    target_unit: Some("eV".into()),
                    provenance_label: Some("unit-test".into()),
                },
                &execution,
                &[candidate("pending-a", 0.3), candidate("pending-b", 0.4)],
                &projector,
                &ScoringStub,
            )
            .expect("scoring execution");

        assert_eq!(result.observations.len(), 3);
        assert_eq!(
            result.feature_representation.family,
            "simple_structure_statistics"
        );
        assert_eq!(result.scoring_request.training_rows.len(), 3);
        assert_eq!(result.scoring_request.candidate_rows.len(), 2);
        assert_eq!(result.scoring_response.selected_model_name, "stub-svgp");
        assert_eq!(
            result.scoring_request.context.objective,
            LearningObjective::ReduceUncertainty
        );
        assert_eq!(result.acquisitions.len(), 2);
        assert_eq!(result.acquisitions[0].rank, 1);
        assert_eq!(result.acquisitions[0].direction, TaskDirection::Downstream);
        assert_eq!(
            result.acquisitions[0].recommended_family,
            Some(BranchFamily::GeneticAlgorithm)
        );
    }

    #[test]
    fn scoring_service_rejects_insufficient_unique_training_observations() {
        let service = GaEmulateScoringService;
        let mut execution = sample_execution();
        execution.generation_populations =
            vec![vec![member("dup", 0.0, -1.0), member("dup", 0.0, -1.0)]];
        let error = service
            .score_pending_candidates(
                &GaEmulateScoringRequest {
                    campaign_id: CampaignId("campaign-1".into()),
                    branch_id: BranchId("branch-ga-1".into()),
                    objective: LearningObjective::ScoreCandidates,
                    fidelity: FidelityClass::JanusMaceLow,
                    surrogate: SurrogateConfig::default(),
                    target_name: "energy".into(),
                    target_unit: Some("eV".into()),
                    provenance_label: None,
                },
                &execution,
                &[candidate("pending-a", 0.3)],
                &SimpleStructureStatisticsProjector,
                &ScoringStub,
            )
            .expect_err("insufficient training rows should fail");
        assert!(error
            .to_string()
            .contains("at least three unique training observations"));
    }

    #[test]
    fn loader_rebuilds_training_observations_from_ga_run_dir() {
        let dir = tempfile::tempdir().expect("tempdir");
        let raw_dir = dir.path().join("raw");
        std::fs::create_dir_all(&raw_dir).expect("raw dir");
        let state = patina_types::GaGenerationState {
            generation: 3,
            population: vec![
                patina_types::GaMemberState {
                    member_id: 0,
                    origin: "MUTATE".into(),
                    occurrences: 1,
                    source: patina_types::StructureRecord::from(&candidate("source-a", 0.0)),
                    evaluation: patina_types::EvaluationRecord::from(
                        &member("a", 0.1, -1.0).result,
                    ),
                    lineage: None,
                    topology: patina_types::GaMemberTopologyRecord::default(),
                },
                patina_types::GaMemberState {
                    member_id: 1,
                    origin: "MUTATE".into(),
                    occurrences: 1,
                    source: patina_types::StructureRecord::from(&candidate("source-b", 0.2)),
                    evaluation: patina_types::EvaluationRecord::from(
                        &member("b", 0.2, -2.0).result,
                    ),
                    lineage: None,
                    topology: patina_types::GaMemberTopologyRecord::default(),
                },
            ],
            elites: Vec::new(),
            repopulation: Vec::new(),
        };
        std::fs::write(
            raw_dir.join("generation_0003_state.json"),
            serde_json::to_string_pretty(&state).expect("serialize state"),
        )
        .expect("write state");

        let observations = load_ga_training_observations_from_run_dir(
            &CampaignId("campaign-1".into()),
            &BranchId("branch-ga-1".into()),
            FidelityClass::JanusMaceLow,
            dir.path(),
        )
        .expect("load observations");

        assert_eq!(observations.len(), 2);
        assert_eq!(observations[0].provenance.generation, Some(3));
        assert_eq!(
            observations[0].provenance.fidelity,
            Some(FidelityClass::JanusMaceLow)
        );
    }
}
