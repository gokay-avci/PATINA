use anyhow::{anyhow, Result};
use patina_emulate::{
    BranchFamily, BranchId, CampaignId, CandidateObservation, FeatureProjectionPort, FidelityClass,
    LearningObjective, ObservationOutcome, ObservationProvenance, SurrogateConfig,
    SurrogateScoringPort,
};
use patina_types::{
    EvalResult, EvaluationRecord, StructureRecord, WorkerOutcome, WorkerRequest, WorkerResponse,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;
use std::time::Duration;

use super::ga_emulate::{
    GaEmulateScoringRequest, GaEmulateScoringService, IdentifiedCandidateInput,
};
use super::ports::GaEvaluationPort;

#[derive(Debug, Clone)]
pub struct EmulateGatedGaConfig {
    pub campaign_id: CampaignId,
    pub branch_id: BranchId,
    pub objective: LearningObjective,
    pub fidelity: FidelityClass,
    pub surrogate: SurrogateConfig,
    pub target_name: String,
    pub target_unit: Option<String>,
    pub provenance_label: Option<String>,
    pub warmup_generations: usize,
    pub min_training_observations: usize,
    pub uncertainty_threshold: f64,
}

impl EmulateGatedGaConfig {
    pub fn validate(&self) -> Result<()> {
        if self.target_name.trim().is_empty() {
            return Err(anyhow!(
                "emulate-gated GA config target_name must not be empty"
            ));
        }
        if self.min_training_observations < 3 {
            return Err(anyhow!(
                "emulate-gated GA config requires at least three training observations"
            ));
        }
        if !self.uncertainty_threshold.is_finite() || self.uncertainty_threshold < 0.0 {
            return Err(anyhow!(
                "emulate-gated GA uncertainty threshold must be finite and non-negative"
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EmulateGateDecisionKind {
    ExactWarmup,
    ExactInsufficientTraining,
    ExactHighUncertainty,
    ExactFallback,
    EmulatedPrediction,
}

#[derive(Debug, Clone, Serialize)]
pub struct EmulateGateDecisionTrace {
    pub request_id: String,
    pub generation: Option<usize>,
    pub candidate_label: String,
    pub decision: EmulateGateDecisionKind,
    pub training_observation_count_before: usize,
    pub selected_model_name: Option<String>,
    pub feature_family: Option<String>,
    pub feature_version: Option<String>,
    pub acquisition_score: Option<f64>,
    pub uncertainty_score: Option<f64>,
    pub uncertainty_threshold: Option<f64>,
    pub predicted_mean: Option<f64>,
    pub predicted_variance: Option<f64>,
    pub fallback_message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EmulateGateBatchTrace {
    pub generation: Option<usize>,
    pub request_count: usize,
    pub training_observation_count_before: usize,
    pub training_observation_count_after: usize,
    pub exact_count: usize,
    pub emulated_count: usize,
    pub decision_counts: BTreeMap<String, usize>,
    pub selected_model_name: Option<String>,
    pub feature_family: Option<String>,
    pub feature_version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EmulateGateRunSummary {
    pub campaign_id: String,
    pub branch_id: String,
    pub objective: LearningObjective,
    pub fidelity: FidelityClass,
    pub warmup_generations: usize,
    pub min_training_observations: usize,
    pub uncertainty_threshold: f64,
    pub feature_family: Option<String>,
    pub feature_version: Option<String>,
    pub exact_evaluation_count: usize,
    pub exact_warmup_count: usize,
    pub exact_insufficient_training_count: usize,
    pub exact_high_uncertainty_count: usize,
    pub exact_fallback_count: usize,
    pub emulated_prediction_count: usize,
    pub final_training_observation_count: usize,
}

#[derive(Debug, Default)]
struct EmulateGatedGaState {
    exact_observations: Vec<CandidateObservation>,
    seen_structures: BTreeSet<String>,
    decision_traces: Vec<EmulateGateDecisionTrace>,
    batch_traces: Vec<EmulateGateBatchTrace>,
    feature_family: Option<String>,
    feature_version: Option<String>,
}

struct GenerationRecordInput {
    traces: Vec<EmulateGateDecisionTrace>,
    training_count_before: usize,
    selected_model_name: Option<String>,
    feature_family: Option<String>,
    feature_version: Option<String>,
}

pub struct EmulateGatedGaEvaluator<'a> {
    exact_evaluator: &'a dyn GaEvaluationPort,
    feature_projection: &'a dyn FeatureProjectionPort,
    surrogate: &'a dyn SurrogateScoringPort,
    config: EmulateGatedGaConfig,
    scoring_service: GaEmulateScoringService,
    state: Mutex<EmulateGatedGaState>,
}

impl<'a> EmulateGatedGaEvaluator<'a> {
    pub fn new(
        exact_evaluator: &'a dyn GaEvaluationPort,
        feature_projection: &'a dyn FeatureProjectionPort,
        surrogate: &'a dyn SurrogateScoringPort,
        config: EmulateGatedGaConfig,
    ) -> Result<Self> {
        config.validate()?;
        Ok(Self {
            exact_evaluator,
            feature_projection,
            surrogate,
            config,
            scoring_service: GaEmulateScoringService,
            state: Mutex::new(EmulateGatedGaState::default()),
        })
    }

    pub fn decision_traces(&self) -> Vec<EmulateGateDecisionTrace> {
        self.state
            .lock()
            .map(|state| state.decision_traces.clone())
            .unwrap_or_else(|poisoned| poisoned.into_inner().decision_traces.clone())
    }

    pub fn batch_traces(&self) -> Vec<EmulateGateBatchTrace> {
        self.state
            .lock()
            .map(|state| state.batch_traces.clone())
            .unwrap_or_else(|poisoned| poisoned.into_inner().batch_traces.clone())
    }

    pub fn run_summary(&self) -> EmulateGateRunSummary {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let exact_warmup_count = state
            .decision_traces
            .iter()
            .filter(|trace| trace.decision == EmulateGateDecisionKind::ExactWarmup)
            .count();
        let exact_insufficient_training_count = state
            .decision_traces
            .iter()
            .filter(|trace| trace.decision == EmulateGateDecisionKind::ExactInsufficientTraining)
            .count();
        let exact_high_uncertainty_count = state
            .decision_traces
            .iter()
            .filter(|trace| trace.decision == EmulateGateDecisionKind::ExactHighUncertainty)
            .count();
        let exact_fallback_count = state
            .decision_traces
            .iter()
            .filter(|trace| trace.decision == EmulateGateDecisionKind::ExactFallback)
            .count();
        let emulated_prediction_count = state
            .decision_traces
            .iter()
            .filter(|trace| trace.decision == EmulateGateDecisionKind::EmulatedPrediction)
            .count();

        EmulateGateRunSummary {
            campaign_id: self.config.campaign_id.0.clone(),
            branch_id: self.config.branch_id.0.clone(),
            objective: self.config.objective,
            fidelity: self.config.fidelity,
            warmup_generations: self.config.warmup_generations,
            min_training_observations: self.config.min_training_observations,
            uncertainty_threshold: self.config.uncertainty_threshold,
            feature_family: state.feature_family.clone(),
            feature_version: state.feature_version.clone(),
            exact_evaluation_count: exact_warmup_count
                + exact_insufficient_training_count
                + exact_high_uncertainty_count
                + exact_fallback_count,
            exact_warmup_count,
            exact_insufficient_training_count,
            exact_high_uncertainty_count,
            exact_fallback_count,
            emulated_prediction_count,
            final_training_observation_count: state.exact_observations.len(),
        }
    }
}

impl GaEvaluationPort for EmulateGatedGaEvaluator<'_> {
    fn evaluate_generation(&self, requests: &[WorkerRequest]) -> Result<Vec<WorkerResponse>> {
        if requests.is_empty() {
            return Ok(Vec::new());
        }

        let (training_observations, training_count_before) = {
            let state = self
                .state
                .lock()
                .map_err(|_| anyhow!("emulate-gated GA state mutex poisoned"))?;
            (
                state.exact_observations.clone(),
                state.exact_observations.len(),
            )
        };

        let generation = requests
            .iter()
            .filter_map(|request| request.generation)
            .min()
            .unwrap_or(0);
        if generation < self.config.warmup_generations {
            return self.evaluate_exact_only(
                requests,
                training_count_before,
                EmulateGateDecisionKind::ExactWarmup,
                None,
            );
        }
        if training_observations.len() < self.config.min_training_observations {
            return self.evaluate_exact_only(
                requests,
                training_count_before,
                EmulateGateDecisionKind::ExactInsufficientTraining,
                None,
            );
        }

        let identified_candidates = requests
            .iter()
            .map(|request| IdentifiedCandidateInput {
                candidate_id: request.request_id.clone(),
                candidate: request.candidate.clone(),
                metadata: [("label".into(), request.candidate.label.clone())]
                    .into_iter()
                    .collect(),
            })
            .collect::<Vec<_>>();

        let scoring_execution = match self
            .scoring_service
            .score_identified_candidates_from_observations(
                &GaEmulateScoringRequest {
                    campaign_id: self.config.campaign_id.clone(),
                    branch_id: self.config.branch_id.clone(),
                    objective: self.config.objective,
                    fidelity: self.config.fidelity,
                    surrogate: self.config.surrogate.clone(),
                    target_name: self.config.target_name.clone(),
                    target_unit: self.config.target_unit.clone(),
                    provenance_label: self.config.provenance_label.clone(),
                },
                training_observations,
                &identified_candidates,
                self.feature_projection,
                self.surrogate,
            ) {
            Ok(execution) => execution,
            Err(error) => {
                return self.evaluate_exact_only(
                    requests,
                    training_count_before,
                    EmulateGateDecisionKind::ExactFallback,
                    Some(error.to_string()),
                );
            }
        };

        let mut predictions_by_id = scoring_execution
            .scoring_response
            .predictions
            .iter()
            .map(|prediction| (prediction.candidate_id.as_str(), prediction))
            .collect::<BTreeMap<_, _>>();
        let mut response_slots = vec![None; requests.len()];
        let mut exact_requests = Vec::new();
        let mut exact_indices = Vec::new();
        let mut traces = Vec::with_capacity(requests.len());
        let selected_model_name = Some(
            scoring_execution
                .scoring_response
                .selected_model_name
                .clone(),
        );
        let feature_family = Some(scoring_execution.feature_representation.family.clone());
        let feature_version = Some(scoring_execution.feature_representation.version.clone());

        for (index, request) in requests.iter().enumerate() {
            let prediction = predictions_by_id
                .remove(request.request_id.as_str())
                .ok_or_else(|| {
                    anyhow!(
                        "surrogate response missing prediction for request `{}`",
                        request.request_id
                    )
                })?;
            let predicted_mean = prediction.means.first().copied().unwrap_or(0.0);
            let predicted_variance = prediction.variances.first().copied().unwrap_or(0.0);
            if prediction.uncertainty_score > self.config.uncertainty_threshold {
                exact_indices.push(index);
                exact_requests.push(request.clone());
                traces.push(EmulateGateDecisionTrace {
                    request_id: request.request_id.clone(),
                    generation: request.generation,
                    candidate_label: request.candidate.label.clone(),
                    decision: EmulateGateDecisionKind::ExactHighUncertainty,
                    training_observation_count_before: training_count_before,
                    selected_model_name: selected_model_name.clone(),
                    feature_family: feature_family.clone(),
                    feature_version: feature_version.clone(),
                    acquisition_score: Some(prediction.acquisition_score),
                    uncertainty_score: Some(prediction.uncertainty_score),
                    uncertainty_threshold: Some(self.config.uncertainty_threshold),
                    predicted_mean: Some(predicted_mean),
                    predicted_variance: Some(predicted_variance),
                    fallback_message: None,
                });
            } else {
                response_slots[index] = Some(emulated_response(request, predicted_mean));
                traces.push(EmulateGateDecisionTrace {
                    request_id: request.request_id.clone(),
                    generation: request.generation,
                    candidate_label: request.candidate.label.clone(),
                    decision: EmulateGateDecisionKind::EmulatedPrediction,
                    training_observation_count_before: training_count_before,
                    selected_model_name: selected_model_name.clone(),
                    feature_family: feature_family.clone(),
                    feature_version: feature_version.clone(),
                    acquisition_score: Some(prediction.acquisition_score),
                    uncertainty_score: Some(prediction.uncertainty_score),
                    uncertainty_threshold: Some(self.config.uncertainty_threshold),
                    predicted_mean: Some(predicted_mean),
                    predicted_variance: Some(predicted_variance),
                    fallback_message: None,
                });
            }
        }

        let exact_responses = self.exact_evaluator.evaluate_generation(&exact_requests)?;
        if exact_responses.len() != exact_requests.len() {
            return Err(anyhow!(
                "exact evaluator returned {} responses for {} emulate-gated requests",
                exact_responses.len(),
                exact_requests.len()
            ));
        }
        for (slot_index, response) in exact_indices.into_iter().zip(exact_responses.iter()) {
            response_slots[slot_index] = Some(response.clone());
        }

        let responses = response_slots
            .into_iter()
            .map(|response| {
                response.ok_or_else(|| anyhow!("missing emulate-gated GA response slot"))
            })
            .collect::<Result<Vec<_>>>()?;
        self.record_generation(
            requests,
            &responses,
            GenerationRecordInput {
                traces,
                training_count_before,
                selected_model_name,
                feature_family,
                feature_version,
            },
        )?;
        Ok(responses)
    }
}

impl EmulateGatedGaEvaluator<'_> {
    fn evaluate_exact_only(
        &self,
        requests: &[WorkerRequest],
        training_count_before: usize,
        decision: EmulateGateDecisionKind,
        fallback_message: Option<String>,
    ) -> Result<Vec<WorkerResponse>> {
        let responses = self.exact_evaluator.evaluate_generation(requests)?;
        if responses.len() != requests.len() {
            return Err(anyhow!(
                "exact evaluator returned {} responses for {} requests",
                responses.len(),
                requests.len()
            ));
        }
        let traces = requests
            .iter()
            .map(|request| EmulateGateDecisionTrace {
                request_id: request.request_id.clone(),
                generation: request.generation,
                candidate_label: request.candidate.label.clone(),
                decision,
                training_observation_count_before: training_count_before,
                selected_model_name: None,
                feature_family: None,
                feature_version: None,
                acquisition_score: None,
                uncertainty_score: None,
                uncertainty_threshold: (decision == EmulateGateDecisionKind::ExactFallback)
                    .then_some(self.config.uncertainty_threshold),
                predicted_mean: None,
                predicted_variance: None,
                fallback_message: fallback_message.clone(),
            })
            .collect::<Vec<_>>();
        self.record_generation(
            requests,
            &responses,
            GenerationRecordInput {
                traces,
                training_count_before,
                selected_model_name: None,
                feature_family: None,
                feature_version: None,
            },
        )?;
        Ok(responses)
    }

    fn record_generation(
        &self,
        requests: &[WorkerRequest],
        responses: &[WorkerResponse],
        record: GenerationRecordInput,
    ) -> Result<()> {
        let GenerationRecordInput {
            traces,
            training_count_before,
            selected_model_name,
            feature_family,
            feature_version,
        } = record;
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow!("emulate-gated GA state mutex poisoned"))?;
        if feature_family.is_some() {
            state.feature_family = feature_family.clone();
        }
        if feature_version.is_some() {
            state.feature_version = feature_version.clone();
        }
        let exact_request_ids = traces
            .iter()
            .filter(|trace| trace.decision != EmulateGateDecisionKind::EmulatedPrediction)
            .map(|trace| trace.request_id.clone())
            .collect::<BTreeSet<_>>();
        append_exact_observations(
            &mut state,
            &self.config,
            requests,
            responses,
            &exact_request_ids,
        )?;

        let mut decision_counts = BTreeMap::new();
        let mut exact_count = 0usize;
        let mut emulated_count = 0usize;
        for trace in &traces {
            *decision_counts
                .entry(decision_label(trace.decision).to_string())
                .or_insert(0) += 1;
            if trace.decision == EmulateGateDecisionKind::EmulatedPrediction {
                emulated_count += 1;
            } else {
                exact_count += 1;
            }
        }
        let generation = requests
            .iter()
            .filter_map(|request| request.generation)
            .min();
        let training_observation_count_after = state.exact_observations.len();
        state.batch_traces.push(EmulateGateBatchTrace {
            generation,
            request_count: requests.len(),
            training_observation_count_before: training_count_before,
            training_observation_count_after,
            exact_count,
            emulated_count,
            decision_counts,
            selected_model_name,
            feature_family,
            feature_version,
        });
        state.decision_traces.extend(traces);
        Ok(())
    }
}

fn append_exact_observations(
    state: &mut EmulateGatedGaState,
    config: &EmulateGatedGaConfig,
    requests: &[WorkerRequest],
    responses: &[WorkerResponse],
    exact_request_ids: &BTreeSet<String>,
) -> Result<()> {
    for (request, response) in requests.iter().zip(responses.iter()) {
        if !exact_request_ids.contains(request.request_id.as_str()) {
            continue;
        }
        let WorkerOutcome::Success { result } = &response.outcome else {
            continue;
        };
        let evaluation = EvaluationRecord::from(result);
        let structure_key = serde_json::to_string(&evaluation.structure)?;
        if !state.seen_structures.insert(structure_key) {
            continue;
        }
        state.exact_observations.push(CandidateObservation {
            campaign_id: config.campaign_id.clone(),
            branch_id: config.branch_id.clone(),
            controller_family: BranchFamily::GeneticAlgorithm,
            source_candidate: StructureRecord::from(&request.candidate),
            evaluated_candidate: Some(evaluation),
            outcome: ObservationOutcome::Accepted,
            provenance: ObservationProvenance {
                branch_id: config.branch_id.clone(),
                family: BranchFamily::GeneticAlgorithm,
                source_label: request.candidate.label.clone(),
                step: None,
                generation: request.generation,
                fidelity: Some(config.fidelity),
                descriptor_provenance: None,
            },
            feature_representation: None,
            feature_vector_ref: None,
        });
    }
    Ok(())
}

fn emulated_response(request: &WorkerRequest, predicted_mean: f64) -> WorkerResponse {
    WorkerResponse {
        request_id: request.request_id.clone(),
        generation: request.generation,
        worker_slot: None,
        outcome: WorkerOutcome::Success {
            result: EvalResult {
                energy: predicted_mean,
                forces: vec![[0.0, 0.0, 0.0]; request.candidate.len()],
                relaxed_candidate: request.candidate.clone(),
                converged: true,
                wall_time: Duration::from_secs(0),
            },
        },
    }
}

fn decision_label(decision: EmulateGateDecisionKind) -> &'static str {
    match decision {
        EmulateGateDecisionKind::ExactWarmup => "exact_warmup",
        EmulateGateDecisionKind::ExactInsufficientTraining => "exact_insufficient_training",
        EmulateGateDecisionKind::ExactHighUncertainty => "exact_high_uncertainty",
        EmulateGateDecisionKind::ExactFallback => "exact_fallback",
        EmulateGateDecisionKind::EmulatedPrediction => "emulated_prediction",
    }
}

#[cfg(test)]
mod tests {
    use super::{EmulateGateDecisionKind, EmulateGatedGaConfig, EmulateGatedGaEvaluator};
    use crate::application::ports::GaEvaluationPort;
    use patina_emulate::{
        BranchFamily, BranchId, CampaignId, FeatureProjectorKind, SurrogateCandidatePrediction,
        SurrogateConfig, SurrogateModelVariant, SurrogateObjective, SurrogateScoringPort,
        SurrogateScoringRequest, SurrogateScoringResponse, SurrogateTask, TaskDirection,
    };
    use patina_types::{Candidate, EvalResult, WorkerOutcome, WorkerRequest, WorkerResponse};
    use std::sync::Mutex;
    use std::time::Duration;

    fn candidate(label: &str, offset: f64) -> Candidate {
        Candidate::cluster(
            label,
            vec!["Mg".into(), "O".into()],
            vec![[offset, 0.0, 0.0], [offset + 1.0, 0.5, 0.0]],
        )
    }

    #[derive(Default)]
    struct ExactEvaluatorStub {
        call_sizes: Mutex<Vec<usize>>,
    }

    impl GaEvaluationPort for ExactEvaluatorStub {
        fn evaluate_generation(
            &self,
            requests: &[WorkerRequest],
        ) -> anyhow::Result<Vec<WorkerResponse>> {
            self.call_sizes
                .lock()
                .expect("call sizes lock")
                .push(requests.len());
            Ok(requests
                .iter()
                .map(|request| WorkerResponse {
                    request_id: request.request_id.clone(),
                    generation: request.generation,
                    worker_slot: None,
                    outcome: WorkerOutcome::Success {
                        result: EvalResult {
                            energy: -(request.candidate.fractional_coords[0][0] + 1.0),
                            forces: vec![[0.0, 0.0, 0.0]; request.candidate.len()],
                            relaxed_candidate: request.candidate.clone(),
                            converged: true,
                            wall_time: Duration::from_secs(1),
                        },
                    },
                })
                .collect())
        }
    }

    struct SurrogateStub;

    impl SurrogateScoringPort for SurrogateStub {
        fn score_candidates(
            &self,
            request: &SurrogateScoringRequest,
        ) -> anyhow::Result<SurrogateScoringResponse> {
            let mut predictions = request
                .candidate_rows
                .iter()
                .enumerate()
                .map(|(index, row)| {
                    let label = row.metadata.get("label").cloned().unwrap_or_default();
                    let uncertainty_score = if label.contains("uncertain") {
                        0.9
                    } else {
                        0.1
                    };
                    SurrogateCandidatePrediction {
                        candidate_id: row.candidate_id.clone(),
                        means: vec![-10.0 - index as f64],
                        variances: vec![uncertainty_score * 0.5],
                        acquisition_score: 1.0 - index as f64 * 0.1,
                        uncertainty_score,
                        rank: index + 1,
                    }
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
                selected_model_name: "stub-whitened-svgp".into(),
                incumbent_target: -12.0,
                checkpoint_path: None,
                predictions,
            })
        }
    }

    #[test]
    fn emulate_gated_evaluator_emulates_low_uncertainty_and_keeps_exact_training() {
        let exact = ExactEvaluatorStub::default();
        let projector =
            patina_emulate::build_feature_projector(FeatureProjectorKind::PairDistanceSignature);
        let evaluator = EmulateGatedGaEvaluator::new(
            &exact,
            &*projector,
            &SurrogateStub,
            EmulateGatedGaConfig {
                campaign_id: CampaignId("campaign-ga".into()),
                branch_id: BranchId("branch-ga".into()),
                objective: patina_emulate::LearningObjective::ReduceUncertainty,
                fidelity: patina_emulate::FidelityClass::GulpReference,
                surrogate: SurrogateConfig {
                    model_variant: SurrogateModelVariant::WhitenedSvgp,
                    objective: SurrogateObjective::Minimize,
                    ..SurrogateConfig::default()
                },
                target_name: "energy".into(),
                target_unit: Some("eV".into()),
                provenance_label: Some("unit-test".into()),
                warmup_generations: 0,
                min_training_observations: 3,
                uncertainty_threshold: 0.5,
            },
        )
        .expect("gated evaluator");

        let warmup_requests = vec![
            WorkerRequest {
                request_id: "warmup-0".into(),
                generation: Some(0),
                candidate: candidate("warmup-a", 0.0),
            },
            WorkerRequest {
                request_id: "warmup-1".into(),
                generation: Some(0),
                candidate: candidate("warmup-b", 1.0),
            },
            WorkerRequest {
                request_id: "warmup-2".into(),
                generation: Some(0),
                candidate: candidate("warmup-c", 2.0),
            },
        ];
        let warmup_responses = evaluator
            .evaluate_generation(&warmup_requests)
            .expect("warmup evaluation");
        assert_eq!(warmup_responses.len(), 3);

        let gated_requests = vec![
            WorkerRequest {
                request_id: "gated-0".into(),
                generation: Some(1),
                candidate: candidate("predict-low", 3.0),
            },
            WorkerRequest {
                request_id: "gated-1".into(),
                generation: Some(1),
                candidate: candidate("predict-uncertain", 4.0),
            },
        ];
        let gated_responses = evaluator
            .evaluate_generation(&gated_requests)
            .expect("gated evaluation");

        assert_eq!(gated_responses.len(), 2);
        match &gated_responses[0].outcome {
            WorkerOutcome::Success { result } => {
                assert_eq!(result.energy, -10.0);
                assert_eq!(result.wall_time, Duration::from_secs(0));
            }
            WorkerOutcome::Failure { .. } => panic!("expected emulated success response"),
        }
        match &gated_responses[1].outcome {
            WorkerOutcome::Success { result } => {
                assert!(result.wall_time >= Duration::from_secs(1));
            }
            WorkerOutcome::Failure { .. } => panic!("expected exact success response"),
        }

        let call_sizes = exact.call_sizes.lock().expect("call sizes").clone();
        assert_eq!(call_sizes, vec![3, 1]);

        let traces = evaluator.decision_traces();
        assert_eq!(traces.len(), 5);
        assert!(traces
            .iter()
            .any(|trace| trace.decision == EmulateGateDecisionKind::EmulatedPrediction));
        assert!(traces
            .iter()
            .any(|trace| trace.decision == EmulateGateDecisionKind::ExactHighUncertainty));

        let summary = evaluator.run_summary();
        assert_eq!(summary.exact_insufficient_training_count, 3);
        assert_eq!(summary.exact_high_uncertainty_count, 1);
        assert_eq!(summary.emulated_prediction_count, 1);
        assert_eq!(summary.final_training_observation_count, 4);
    }
}
