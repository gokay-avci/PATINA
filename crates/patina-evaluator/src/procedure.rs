use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use patina_types::Candidate;

use crate::plan::ScottProcedurePlan;
use crate::runtime::{ProcedureAction, ProcedureCursor, ProcedureRuntimeState, StageTicket};
use crate::stages::{StageSelectionError, StageTransition};
use crate::status::{
    normalize_backend_attempt, BackendAttemptReport, ProcedureDecision, ScottEvalOutcome,
    ScottEvalRollback, ScottEvaluationState, ScottProcedureGateEvidence, ScottProcedureGateKind,
    ScottProcedureGateOutcome, StageBackendStatus,
};

/// Procedure request passed from application workflows into the Scott evaluator core.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScottProcedureRequest {
    pub candidate: Candidate,
    pub request_id: String,
    pub workdir: Utf8PathBuf,
}

/// Errors returned by the Scott evaluator procedure boundary.
#[derive(Debug, Error)]
pub enum ScottEvaluatorError {
    #[error("procedure plan contains no stages")]
    EmptyStagePlan,
    #[error("invalid stage plan: {0:?}")]
    InvalidStagePlan(StageSelectionError),
    #[error("candidate validation failed: {0:?}")]
    InvalidCandidate(patina_types::CandidateError),
    #[error("attempt report stage {found} does not match expected stage {expected}")]
    StageMismatch { expected: u8, found: u8 },
    #[error("stage retrieval completed while procedure state was {0:?}")]
    InvalidRetrievalState(ProcedureRuntimeState),
    #[error("scott evaluator procedure is not implemented yet: {0}")]
    NotImplemented(&'static str),
}

/// Trait for the Scott-native evaluator procedure.
///
/// This sits above concrete backend adapters and owns Scott procedure semantics such as
/// staged evaluation, retry/rollback policy, and retrieval timing.
pub trait ScottEvaluator: Send + Sync {
    fn evaluate(
        &self,
        plan: &ScottProcedurePlan,
        request: &ScottProcedureRequest,
    ) -> Result<ScottEvalOutcome, ScottEvaluatorError>;
}

/// Placeholder implementation used while the full native procedure is migrated.
#[derive(Debug, Default, Clone, Copy)]
pub struct ScottEvaluatorProcedure;

/// Mutable procedure state for one candidate as backend attempt reports arrive.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcedureState {
    pub cursor: ProcedureCursor,
    pub evaluation_state: ScottEvaluationState,
    working_candidate: Candidate,
    pending_retrieved_candidate: Option<Candidate>,
    pending_retrieval: Option<PendingRetrieval>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum PendingRetrieval {
    AcceptedStage {
        stage: crate::stages::StageIndex,
        accepted_energy: Option<f64>,
    },
    RetryCurrentStage,
}

impl ProcedureState {
    pub fn new(
        plan: &ScottProcedurePlan,
        request: &ScottProcedureRequest,
    ) -> Result<Self, ScottEvaluatorError> {
        let first_stage = plan
            .stages
            .first()
            .ok_or(ScottEvaluatorError::EmptyStagePlan)?
            .stage;
        let ticket = StageTicket {
            request_id: request.request_id.clone(),
            backend_mode: plan.evaluator.settings.backend_mode,
            stage: first_stage,
            attempt: 1,
            workdir: request.workdir.clone(),
        };

        Ok(Self {
            cursor: ProcedureCursor::new(ticket),
            evaluation_state: ScottEvaluationState::new(
                request.request_id.clone(),
                request.candidate.clone(),
                first_stage,
            ),
            working_candidate: request.candidate.clone(),
            pending_retrieved_candidate: None,
            pending_retrieval: None,
        })
    }

    pub fn current_candidate(&self) -> &Candidate {
        &self.working_candidate
    }

    pub fn next_action(&self) -> ProcedureAction {
        match self.cursor.state {
            ProcedureRuntimeState::ReadyToSubmit => self.cursor.submit_action(),
            ProcedureRuntimeState::Submitted | ProcedureRuntimeState::Running => {
                self.cursor.poll_action()
            }
            ProcedureRuntimeState::Retrieving | ProcedureRuntimeState::StageAccepted => {
                self.cursor.retrieve_action()
            }
            ProcedureRuntimeState::Completed => ProcedureAction::FinishAccepted,
            ProcedureRuntimeState::Rejected => ProcedureAction::FinishRejected,
            ProcedureRuntimeState::Failed => ProcedureAction::FinishFailed,
        }
    }

    pub fn apply_attempt_report(
        &mut self,
        plan: &ScottProcedurePlan,
        report: BackendAttemptReport,
    ) -> Result<ProcedureAction, ScottEvaluatorError> {
        let current_stage = self.cursor.ticket.stage;
        if report.stage != current_stage {
            return Err(ScottEvaluatorError::StageMismatch {
                expected: current_stage.get(),
                found: report.stage.get(),
            });
        }
        let (report, procedure_gate_evidence) = apply_scott_acceptance_gates(plan, report);

        let is_final_stage = plan.stages.next_after(current_stage).is_none();
        let (status, failure, decision) = normalize_backend_attempt(
            &report,
            plan.evaluator.settings.max_relaxation_attempts,
            plan.evaluator.settings.final_stage_failure_policy,
            is_final_stage,
        );

        self.evaluation_state
            .record_stage_evaluation(status, report.result.clone());
        for evidence in procedure_gate_evidence {
            self.evaluation_state
                .record_procedure_gate_evidence(evidence);
        }
        if let Some(failure) = failure {
            self.evaluation_state.record_failure(failure);
        }

        let action = match decision {
            ProcedureDecision::AcceptStage => {
                let accepted_energy = accepted_energy(&report);
                if let Some(result) = report.result.clone() {
                    self.evaluation_state
                        .set_accepted_result(report.stage, result);
                }
                if plan.evaluator.settings.retrieve_relaxed_geometry {
                    if let Some(result) = report.result.as_ref() {
                        self.pending_retrieved_candidate = Some(result.relaxed_candidate.clone());
                        self.pending_retrieval = Some(PendingRetrieval::AcceptedStage {
                            stage: report.stage,
                            accepted_energy,
                        });
                        self.cursor.state = ProcedureRuntimeState::Retrieving;
                        ProcedureAction::Retrieve(self.cursor.ticket.clone())
                    } else {
                        self.finish_accepted_stage(plan, report.stage, accepted_energy)?
                    }
                } else {
                    self.finish_accepted_stage(plan, report.stage, accepted_energy)?
                }
            }
            ProcedureDecision::RetryStage => {
                if plan.evaluator.settings.retrieve_relaxed_geometry {
                    if let Some(result) = report.result.as_ref() {
                        self.pending_retrieved_candidate = Some(result.relaxed_candidate.clone());
                        self.pending_retrieval = Some(PendingRetrieval::RetryCurrentStage);
                        self.cursor.state = ProcedureRuntimeState::Retrieving;
                        ProcedureAction::Retrieve(self.cursor.ticket.clone())
                    } else {
                        self.cursor.ticket.attempt += 1;
                        self.cursor.state = ProcedureRuntimeState::ReadyToSubmit;
                        ProcedureAction::Retry(self.cursor.ticket.clone())
                    }
                } else {
                    self.cursor.ticket.attempt += 1;
                    self.cursor.state = ProcedureRuntimeState::ReadyToSubmit;
                    ProcedureAction::Retry(self.cursor.ticket.clone())
                }
            }
            ProcedureDecision::RejectCandidate => {
                self.evaluation_state
                    .mark_relax_failed(ScottEvalRollback::for_failed_stage(
                        report.stage,
                        self.evaluation_state.accepted_stage,
                        plan.evaluator.settings.final_stage_failure_policy,
                    ));
                self.cursor.state = ProcedureRuntimeState::Rejected;
                ProcedureAction::FinishRejected
            }
            ProcedureDecision::KeepPreviousAccepted => {
                self.evaluation_state
                    .mark_relax_failed(ScottEvalRollback::for_failed_stage(
                        report.stage,
                        self.evaluation_state.accepted_stage,
                        plan.evaluator.settings.final_stage_failure_policy,
                    ));
                self.cursor.state = ProcedureRuntimeState::Completed;
                ProcedureAction::FinishAccepted
            }
        };

        Ok(action)
    }

    pub fn complete_retrieval(
        &mut self,
        plan: &ScottProcedurePlan,
    ) -> Result<ProcedureAction, ScottEvaluatorError> {
        if !matches!(
            self.cursor.state,
            ProcedureRuntimeState::Retrieving | ProcedureRuntimeState::StageAccepted
        ) {
            return Err(ScottEvaluatorError::InvalidRetrievalState(
                self.cursor.state.clone(),
            ));
        }

        let current_stage = self.cursor.ticket.stage;
        if let Some(candidate) = self.pending_retrieved_candidate.take() {
            self.working_candidate = candidate;
        }

        match self.pending_retrieval.take() {
            Some(PendingRetrieval::AcceptedStage {
                stage,
                accepted_energy,
            }) => self.finish_accepted_stage(plan, stage, accepted_energy),
            Some(PendingRetrieval::RetryCurrentStage) => {
                self.cursor.ticket.attempt += 1;
                self.cursor.state = ProcedureRuntimeState::ReadyToSubmit;
                Ok(ProcedureAction::Retry(self.cursor.ticket.clone()))
            }
            None => {
                let accepted_energy = self
                    .evaluation_state
                    .accepted_result
                    .as_ref()
                    .filter(|_| self.evaluation_state.accepted_stage == Some(current_stage))
                    .map(|result| result.energy)
                    .or_else(|| {
                        self.evaluation_state
                            .stage_evaluations
                            .iter()
                            .rev()
                            .find(|entry| entry.status.stage == current_stage)
                            .and_then(|entry| entry.status.energy)
                    });
                self.finish_accepted_stage(plan, current_stage, accepted_energy)
            }
        }
    }

    pub fn into_outcome(self) -> ScottEvalOutcome {
        let provenance = self.evaluation_state.provenance();
        let final_result = self.evaluation_state.accepted_result.clone();
        let final_stage = self.evaluation_state.accepted_stage;
        let relax_failed = self.evaluation_state.relax_failed;
        ScottEvalOutcome {
            final_result,
            final_stage,
            relax_failed,
            provenance,
            state: self.evaluation_state,
        }
    }

    fn finish_accepted_stage(
        &mut self,
        plan: &ScottProcedurePlan,
        accepted_stage: crate::stages::StageIndex,
        accepted_energy: Option<f64>,
    ) -> Result<ProcedureAction, ScottEvaluatorError> {
        let transition = accepted_energy
            .map(|energy| {
                plan.stages
                    .transition_for_energy(accepted_stage, energy)
                    .map_err(ScottEvaluatorError::InvalidStagePlan)
            })
            .transpose()?
            .unwrap_or(StageTransition::Stop);

        match transition {
            StageTransition::Continue => {
                if let Some(next_stage) = plan.stages.next_after(accepted_stage) {
                    self.cursor.ticket.stage = next_stage.stage;
                    self.cursor.ticket.attempt = 1;
                    self.cursor.state = ProcedureRuntimeState::ReadyToSubmit;
                    self.evaluation_state.current_stage = Some(next_stage.stage);
                    Ok(ProcedureAction::AdvanceTo(next_stage.stage))
                } else {
                    self.cursor.state = ProcedureRuntimeState::Completed;
                    Ok(ProcedureAction::FinishAccepted)
                }
            }
            StageTransition::Stop => {
                self.cursor.state = ProcedureRuntimeState::Completed;
                Ok(ProcedureAction::FinishAccepted)
            }
        }
    }
}

fn accepted_energy(report: &BackendAttemptReport) -> Option<f64> {
    report
        .energy
        .or_else(|| report.result.as_ref().map(|result| result.energy))
}

fn apply_scott_acceptance_gates(
    plan: &ScottProcedurePlan,
    mut report: BackendAttemptReport,
) -> (BackendAttemptReport, Vec<ScottProcedureGateEvidence>) {
    let mut evidence = Vec::new();
    if !matches!(
        report.backend_status,
        StageBackendStatus::Converged | StageBackendStatus::ConvergedWithGradientWarning
    ) {
        return (report, evidence);
    }

    let Some(stage_plan) = plan.stages.get(report.stage) else {
        return (report, evidence);
    };

    if report.result.is_none() {
        report.backend_status = StageBackendStatus::RejectedByProcedure;
        evidence.push(ScottProcedureGateEvidence {
            stage: report.stage,
            attempt: report.attempt,
            kind: ScottProcedureGateKind::MissingRelaxedResult,
            outcome: ScottProcedureGateOutcome::RejectStage,
            message: "stage converged but no relaxed result payload was available".into(),
            threshold: None,
            observed_value: None,
        });
        return (report, evidence);
    }

    if let Some(gnorm) = report.gnorm {
        if gnorm > plan.evaluator.settings.gnorm_tolerance {
            report.backend_status = StageBackendStatus::RequiresMoreCycles;
            evidence.push(ScottProcedureGateEvidence {
                stage: report.stage,
                attempt: report.attempt,
                kind: ScottProcedureGateKind::GradientToleranceExceeded,
                outcome: ScottProcedureGateOutcome::RetryStage,
                message: "gradient norm exceeded Scott tolerance; retrying current stage".into(),
                threshold: Some(plan.evaluator.settings.gnorm_tolerance),
                observed_value: Some(gnorm),
            });
            return (report, evidence);
        }
    }

    let Some(energy) = accepted_energy(&report) else {
        report.backend_status = StageBackendStatus::InvalidEnergy;
        evidence.push(ScottProcedureGateEvidence {
            stage: report.stage,
            attempt: report.attempt,
            kind: ScottProcedureGateKind::MissingAcceptedEnergy,
            outcome: ScottProcedureGateOutcome::RejectStage,
            message: "accepted result did not provide a usable energy".into(),
            threshold: None,
            observed_value: None,
        });
        return (report, evidence);
    };

    let below_min = stage_plan
        .energy_min_threshold
        .map(|threshold| energy < threshold)
        .unwrap_or(false);
    let above_max = stage_plan
        .energy_max_threshold
        .map(|threshold| energy > threshold)
        .unwrap_or(false);
    if below_min {
        report.backend_status = StageBackendStatus::RejectedByProcedure;
        evidence.push(ScottProcedureGateEvidence {
            stage: report.stage,
            attempt: report.attempt,
            kind: ScottProcedureGateKind::EnergyBelowStageMinimum,
            outcome: ScottProcedureGateOutcome::RejectStage,
            message: "stage energy fell below the configured Scott minimum threshold".into(),
            threshold: stage_plan.energy_min_threshold,
            observed_value: Some(energy),
        });
    }
    if above_max {
        report.backend_status = StageBackendStatus::RejectedByProcedure;
        evidence.push(ScottProcedureGateEvidence {
            stage: report.stage,
            attempt: report.attempt,
            kind: ScottProcedureGateKind::EnergyAboveStageMaximum,
            outcome: ScottProcedureGateOutcome::RejectStage,
            message: "stage energy exceeded the configured Scott maximum threshold".into(),
            threshold: stage_plan.energy_max_threshold,
            observed_value: Some(energy),
        });
    }

    (report, evidence)
}

impl ScottEvaluator for ScottEvaluatorProcedure {
    fn evaluate(
        &self,
        plan: &ScottProcedurePlan,
        request: &ScottProcedureRequest,
    ) -> Result<ScottEvalOutcome, ScottEvaluatorError> {
        if plan.stages.is_empty() {
            return Err(ScottEvaluatorError::EmptyStagePlan);
        }
        plan.stages
            .validate()
            .map_err(ScottEvaluatorError::InvalidStagePlan)?;
        request
            .candidate
            .validate()
            .map_err(ScottEvaluatorError::InvalidCandidate)?;
        let _state = ProcedureState::new(plan, request)?;
        Err(ScottEvaluatorError::NotImplemented(
            "native Scott evaluation procedure migration not wired yet",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{ProcedureState, ScottEvaluatorError, ScottProcedureRequest};
    use crate::{
        BackendAttemptReport, FinalStageFailurePolicy, ProcedureAction, ProcedureRuntimeState,
        ScottBackendMode, ScottEvaluatorPlan, ScottEvaluatorSettings, ScottLatticeMode,
        ScottProcedureGateKind, ScottProcedureGateOutcome, ScottProcedureIntent,
        ScottProcedurePlan, StageBackendStatus, StageEngine, StageIndex, StagePlan, StageSelection,
    };
    use patina_types::{Candidate, EvalResult};
    use std::time::Duration;

    fn request() -> ScottProcedureRequest {
        ScottProcedureRequest {
            candidate: Candidate {
                species: vec!["Mg".into()],
                fractional_coords: vec![[0.0, 0.0, 0.0]],
                lattice: None,
                periodic_axes: [false, false, false],
                label: "mg1".into(),
            },
            request_id: "req-1".into(),
            workdir: "/tmp/scott-procedure".into(),
        }
    }

    fn eval_result(label: &str, energy: f64) -> EvalResult {
        EvalResult {
            energy,
            forces: vec![[0.0, 0.0, 0.0]],
            relaxed_candidate: Candidate {
                species: vec!["Mg".into()],
                fractional_coords: vec![[0.0, 0.0, 0.0]],
                lattice: None,
                periodic_axes: [false, false, false],
                label: label.into(),
            },
            converged: true,
            wall_time: Duration::from_secs(1),
        }
    }

    fn plan() -> ScottProcedurePlan {
        ScottProcedurePlan {
            evaluator: ScottEvaluatorPlan {
                master_template: "/tmp/Master.gin".into(),
                atoms_in: Some("/tmp/atoms.in".into()),
                work_root: "/tmp".into(),
                settings: ScottEvaluatorSettings {
                    backend_mode: ScottBackendMode::Gulp,
                    procedure_intent: ScottProcedureIntent::ProductionRun,
                    lattice_mode: ScottLatticeMode::Cluster,
                    max_relaxation_attempts: 2,
                    gnorm_tolerance: 1.0e-4,
                    final_stage_failure_policy: FinalStageFailurePolicy::KeepPreviousAccepted,
                    retrieve_relaxed_geometry: true,
                    keep_stage_artifacts: true,
                },
            },
            stages: StageSelection {
                stages: vec![
                    StagePlan {
                        stage: StageIndex(1),
                        engine: StageEngine::Gulp,
                        refine_if_energy_below: None,
                        energy_min_threshold: None,
                        energy_max_threshold: None,
                        keep_only_if_final_stage: false,
                    },
                    StagePlan {
                        stage: StageIndex(2),
                        engine: StageEngine::Aims,
                        refine_if_energy_below: None,
                        energy_min_threshold: None,
                        energy_max_threshold: None,
                        keep_only_if_final_stage: true,
                    },
                ],
            },
        }
    }

    #[test]
    fn procedure_state_starts_from_first_stage_submit() {
        let state = ProcedureState::new(&plan(), &request()).unwrap();
        assert_eq!(state.cursor.ticket.stage, StageIndex(1));
        assert_eq!(
            state.next_action(),
            ProcedureAction::Submit(state.cursor.ticket.clone())
        );
    }

    #[test]
    fn accepted_stage_advances_to_next_stage() {
        let mut state = ProcedureState::new(&plan(), &request()).unwrap();
        let action = state
            .apply_attempt_report(
                &plan(),
                BackendAttemptReport {
                    stage: StageIndex(1),
                    attempt: 1,
                    backend_status: StageBackendStatus::Converged,
                    failure_detail: None,
                    energy: Some(-10.0),
                    gnorm: Some(1.0e-5),
                    relaxed_label: Some("stage1".into()),
                    primary_output_path: Some("stage_01_attempt_01/gulp_klmc.gout".into()),
                    result: Some(eval_result("stage1", -10.0)),
                },
            )
            .unwrap();

        assert_eq!(
            action,
            ProcedureAction::Retrieve(state.cursor.ticket.clone())
        );
        assert_eq!(state.cursor.ticket.stage, StageIndex(1));
        assert_eq!(state.cursor.state, ProcedureRuntimeState::Retrieving);
        assert_eq!(state.evaluation_state.accepted_stage, Some(StageIndex(1)));
        assert_eq!(state.current_candidate().label, "mg1");

        let action = state.complete_retrieval(&plan()).unwrap();
        assert_eq!(action, ProcedureAction::AdvanceTo(StageIndex(2)));
        assert_eq!(state.cursor.ticket.stage, StageIndex(2));
        assert_eq!(state.cursor.state, ProcedureRuntimeState::ReadyToSubmit);
        assert_eq!(state.evaluation_state.accepted_stage, Some(StageIndex(1)));
        assert_eq!(state.current_candidate().label, "stage1");
    }

    #[test]
    fn refine_threshold_can_stop_before_next_stage() {
        let mut gated_plan = plan();
        gated_plan.stages.stages[0].refine_if_energy_below = Some(-20.0);
        let mut state = ProcedureState::new(&gated_plan, &request()).unwrap();
        let action = state
            .apply_attempt_report(
                &gated_plan,
                BackendAttemptReport {
                    stage: StageIndex(1),
                    attempt: 1,
                    backend_status: StageBackendStatus::Converged,
                    failure_detail: None,
                    energy: Some(-10.0),
                    gnorm: Some(1.0e-5),
                    relaxed_label: Some("stage1".into()),
                    primary_output_path: Some("stage_01_attempt_01/gulp_klmc.gout".into()),
                    result: Some(eval_result("stage1", -10.0)),
                },
            )
            .unwrap();

        assert_eq!(
            action,
            ProcedureAction::Retrieve(state.cursor.ticket.clone())
        );
        let action = state.complete_retrieval(&gated_plan).unwrap();
        assert_eq!(action, ProcedureAction::FinishAccepted);
        assert_eq!(state.evaluation_state.accepted_stage, Some(StageIndex(1)));
        assert_eq!(state.cursor.state, ProcedureRuntimeState::Completed);
    }

    #[test]
    fn accepted_stage_can_skip_retrieval_when_disabled() {
        let mut no_retrieve_plan = plan();
        no_retrieve_plan
            .evaluator
            .settings
            .retrieve_relaxed_geometry = false;
        let mut state = ProcedureState::new(&no_retrieve_plan, &request()).unwrap();
        let action = state
            .apply_attempt_report(
                &no_retrieve_plan,
                BackendAttemptReport {
                    stage: StageIndex(1),
                    attempt: 1,
                    backend_status: StageBackendStatus::Converged,
                    failure_detail: None,
                    energy: Some(-10.0),
                    gnorm: Some(1.0e-5),
                    relaxed_label: Some("stage1".into()),
                    primary_output_path: Some("stage_01_attempt_01/gulp_klmc.gout".into()),
                    result: Some(eval_result("stage1", -10.0)),
                },
            )
            .unwrap();

        assert_eq!(action, ProcedureAction::AdvanceTo(StageIndex(2)));
        assert_eq!(state.cursor.ticket.stage, StageIndex(2));
        assert_eq!(state.cursor.state, ProcedureRuntimeState::ReadyToSubmit);
    }

    #[test]
    fn retryable_failure_increments_attempt() {
        let mut state = ProcedureState::new(&plan(), &request()).unwrap();
        let action = state
            .apply_attempt_report(
                &plan(),
                BackendAttemptReport {
                    stage: StageIndex(1),
                    attempt: 1,
                    backend_status: StageBackendStatus::RequiresMoreCycles,
                    failure_detail: None,
                    energy: Some(-10.0),
                    gnorm: Some(1.0),
                    relaxed_label: None,
                    primary_output_path: Some("stage_01_attempt_01/gulp_klmc.gout".into()),
                    result: None,
                },
            )
            .unwrap();

        match action {
            ProcedureAction::Retry(ticket) => assert_eq!(ticket.attempt, 2),
            other => panic!("expected retry action, got {other:?}"),
        }
        assert_eq!(state.cursor.ticket.attempt, 2);
        assert_eq!(state.current_candidate().label, "mg1");
    }

    #[test]
    fn gnorm_above_tolerance_retries_stage() {
        let mut state = ProcedureState::new(&plan(), &request()).unwrap();
        let action = state
            .apply_attempt_report(
                &plan(),
                BackendAttemptReport {
                    stage: StageIndex(1),
                    attempt: 1,
                    backend_status: StageBackendStatus::Converged,
                    failure_detail: None,
                    energy: Some(-10.0),
                    gnorm: Some(1.0e-2),
                    relaxed_label: Some("stage1".into()),
                    primary_output_path: Some("stage_01_attempt_01/gulp_klmc.gout".into()),
                    result: Some(eval_result("stage1", -10.0)),
                },
            )
            .unwrap();

        assert_eq!(
            action,
            ProcedureAction::Retrieve(state.cursor.ticket.clone())
        );
        assert_eq!(state.cursor.ticket.attempt, 1);
        assert_eq!(state.cursor.state, ProcedureRuntimeState::Retrieving);
        assert!(state.evaluation_state.accepted_result.is_none());
        assert_eq!(state.evaluation_state.procedure_gate_evidence.len(), 1);
        assert_eq!(
            state.evaluation_state.procedure_gate_evidence[0].kind,
            ScottProcedureGateKind::GradientToleranceExceeded
        );
        assert_eq!(
            state.evaluation_state.procedure_gate_evidence[0].outcome,
            ScottProcedureGateOutcome::RetryStage
        );
        let action = state.complete_retrieval(&plan()).unwrap();
        match action {
            ProcedureAction::Retry(ticket) => assert_eq!(ticket.attempt, 2),
            other => panic!("expected retry action after retrieval, got {other:?}"),
        }
        assert_eq!(state.cursor.ticket.attempt, 2);
        assert_eq!(state.cursor.state, ProcedureRuntimeState::ReadyToSubmit);
        assert_eq!(state.current_candidate().label, "stage1");
    }

    #[test]
    fn energy_outside_stage_window_is_rejected_by_procedure() {
        let mut bounded_plan = plan();
        bounded_plan.stages.stages[0].energy_max_threshold = Some(-20.0);
        let mut state = ProcedureState::new(&bounded_plan, &request()).unwrap();
        let action = state
            .apply_attempt_report(
                &bounded_plan,
                BackendAttemptReport {
                    stage: StageIndex(1),
                    attempt: 1,
                    backend_status: StageBackendStatus::Converged,
                    failure_detail: None,
                    energy: Some(-10.0),
                    gnorm: Some(1.0e-5),
                    relaxed_label: Some("stage1".into()),
                    primary_output_path: Some("stage_01_attempt_01/gulp_klmc.gout".into()),
                    result: Some(eval_result("stage1", -10.0)),
                },
            )
            .unwrap();

        assert_eq!(action, ProcedureAction::FinishRejected);
        assert_eq!(state.cursor.state, ProcedureRuntimeState::Rejected);
        assert_eq!(state.evaluation_state.procedure_gate_evidence.len(), 1);
        assert_eq!(
            state.evaluation_state.procedure_gate_evidence[0].kind,
            ScottProcedureGateKind::EnergyAboveStageMaximum
        );
    }

    #[test]
    fn converged_stage_without_result_is_rejected() {
        let mut state = ProcedureState::new(&plan(), &request()).unwrap();
        let action = state
            .apply_attempt_report(
                &plan(),
                BackendAttemptReport {
                    stage: StageIndex(1),
                    attempt: 1,
                    backend_status: StageBackendStatus::Converged,
                    failure_detail: None,
                    energy: Some(-10.0),
                    gnorm: Some(1.0e-5),
                    relaxed_label: None,
                    primary_output_path: Some("stage_01_attempt_01/gulp_klmc.gout".into()),
                    result: None,
                },
            )
            .unwrap();

        assert_eq!(action, ProcedureAction::FinishRejected);
        assert_eq!(state.cursor.state, ProcedureRuntimeState::Rejected);
        assert_eq!(state.evaluation_state.procedure_gate_evidence.len(), 1);
        assert_eq!(
            state.evaluation_state.procedure_gate_evidence[0].kind,
            ScottProcedureGateKind::MissingRelaxedResult
        );
    }

    #[test]
    fn final_stage_failure_can_finish_with_previous_result() {
        let mut state = ProcedureState::new(&plan(), &request()).unwrap();
        state
            .evaluation_state
            .set_accepted_result(StageIndex(1), eval_result("stage1", -10.0));
        state.cursor.ticket.stage = StageIndex(2);

        let action = state
            .apply_attempt_report(
                &plan(),
                BackendAttemptReport {
                    stage: StageIndex(2),
                    attempt: 2,
                    backend_status: StageBackendStatus::Crashed,
                    failure_detail: None,
                    energy: None,
                    gnorm: None,
                    relaxed_label: None,
                    primary_output_path: Some("stage_02_attempt_02/gulp_klmc.gout".into()),
                    result: None,
                },
            )
            .unwrap();

        assert_eq!(action, ProcedureAction::FinishAccepted);
        assert_eq!(state.cursor.state, ProcedureRuntimeState::Completed);
        assert!(state.evaluation_state.relax_failed);
        assert_eq!(
            state
                .evaluation_state
                .rollback
                .as_ref()
                .and_then(|rollback| rollback.preserved_stage),
            Some(StageIndex(1))
        );
        let outcome = state.into_outcome();
        assert_eq!(outcome.final_stage, Some(StageIndex(1)));
        assert!(outcome.final_result.is_some());
        assert_eq!(outcome.state.accepted_stage, Some(StageIndex(1)));
    }

    #[test]
    fn stage_mismatch_is_rejected() {
        let mut state = ProcedureState::new(&plan(), &request()).unwrap();
        let error = state
            .apply_attempt_report(
                &plan(),
                BackendAttemptReport {
                    stage: StageIndex(2),
                    attempt: 1,
                    backend_status: StageBackendStatus::Converged,
                    failure_detail: None,
                    energy: Some(-10.0),
                    gnorm: Some(1.0e-5),
                    relaxed_label: Some("wrong".into()),
                    primary_output_path: Some("stage_02_attempt_01/gulp_klmc.gout".into()),
                    result: Some(eval_result("wrong", -10.0)),
                },
            )
            .unwrap_err();

        assert!(matches!(
            error,
            ScottEvaluatorError::StageMismatch {
                expected: 1,
                found: 2
            }
        ));
    }
}
