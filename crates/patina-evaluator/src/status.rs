use serde::{Deserialize, Serialize};
use thiserror::Error;

use patina_types::{Candidate, EvalResult};

use crate::stages::{FinalStageFailurePolicy, StageIndex};

/// Backend-native status normalized into Scott procedure vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageBackendStatus {
    Converged,
    ConvergedWithGradientWarning,
    RequiresMoreCycles,
    Crashed,
    InvalidEnergy,
    RejectedByProcedure,
}

/// Scott-facing convergence class after native/backend-specific normalization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NormalizedConvergence {
    Accepted,
    RetryableFailure,
    FinalFailure,
}

/// Stage failure record used to preserve rollback semantics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScottStageFailure {
    pub stage: StageIndex,
    pub attempt: usize,
    pub backend_status: StageBackendStatus,
    pub message: String,
}

/// Rollback applied when a Scott stage fails.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScottEvalRollback {
    pub failed_stage: StageIndex,
    pub rollback_to_stage: Option<StageIndex>,
    pub preserved_stage: Option<StageIndex>,
    pub mark_relax_failed: bool,
}

/// Per-stage outcome after status normalization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScottStageStatus {
    pub stage: StageIndex,
    pub attempt: usize,
    pub backend_status: StageBackendStatus,
    pub convergence: NormalizedConvergence,
    pub energy: Option<f64>,
    pub gnorm: Option<f64>,
    pub relaxed_label: Option<String>,
    pub primary_output_path: Option<String>,
}

/// Provenance for the full Scott evaluation procedure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ScottEvalProvenance {
    pub completed_stages: Vec<ScottStageStatus>,
    pub failures: Vec<ScottStageFailure>,
    pub rollback: Option<ScottEvalRollback>,
}

/// Evaluator-owned gate evidence recorded when Scott procedure semantics override raw backend status.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScottProcedureGateEvidence {
    pub stage: StageIndex,
    pub attempt: usize,
    pub kind: ScottProcedureGateKind,
    pub outcome: ScottProcedureGateOutcome,
    pub message: String,
    pub threshold: Option<f64>,
    pub observed_value: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScottProcedureGateKind {
    MissingRelaxedResult,
    GradientToleranceExceeded,
    MissingAcceptedEnergy,
    EnergyBelowStageMinimum,
    EnergyAboveStageMaximum,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScottProcedureGateOutcome {
    RetryStage,
    RejectStage,
}

/// Post-evaluation validity or boundary check recorded against Scott state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScottValidityCheck {
    pub kind: ScottValidityCheckKind,
    pub passed: bool,
    pub message: String,
    pub threshold: Option<f64>,
    pub observed_value: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScottValidityCheckKind {
    Collapse,
    LikeSpeciesTransfer,
    HashkeyAvailable,
}

/// Topology/duplicate-adjacent provenance derived outside the evaluator kernel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ScottTopologyProvenance {
    pub topology_analysis_requested: bool,
    pub topology_skip: bool,
    pub input_hashkey: Option<String>,
    pub final_hashkey: Option<String>,
}

/// Shared science evidence that higher layers can attach to Scott state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ScottScienceEvidence {
    pub validity_checks: Vec<ScottValidityCheck>,
    pub topology: Option<ScottTopologyProvenance>,
    pub duplicate: Option<ScottDuplicateProvenance>,
}

/// Duplicate-identity provenance attached by higher-level workflow controllers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScottDuplicateProvenance {
    pub reason: ScottDuplicateReason,
    pub action: ScottDuplicateAction,
    pub matched_candidate_label: Option<String>,
    pub matched_hashkey: Option<String>,
    pub matched_rank: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScottDuplicateReason {
    Hashkey,
    Pmoi,
    EnergyTolerance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScottDuplicateAction {
    MatchedBestArchiveInputHashkey,
    MatchedBlacklist,
    MatchedImportedLibrary,
    MatchedCurrentRunHistory,
    MatchedExistingBestSet,
}

/// Full record for one Scott stage attempt, including the relaxed structure payload when present.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScottStageEvaluation {
    pub status: ScottStageStatus,
    pub result: Option<EvalResult>,
}

/// Scott-native evaluation state for one candidate across the full staged procedure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScottEvaluationState {
    pub request_id: String,
    pub input_candidate: Candidate,
    pub current_stage: Option<StageIndex>,
    pub accepted_result: Option<EvalResult>,
    pub accepted_stage: Option<StageIndex>,
    pub relax_failed: bool,
    pub stage_evaluations: Vec<ScottStageEvaluation>,
    pub failures: Vec<ScottStageFailure>,
    pub rollback: Option<ScottEvalRollback>,
    pub procedure_gate_evidence: Vec<ScottProcedureGateEvidence>,
    pub science_evidence: ScottScienceEvidence,
}

impl ScottEvaluationState {
    pub fn new(request_id: String, input_candidate: Candidate, initial_stage: StageIndex) -> Self {
        Self {
            request_id,
            input_candidate,
            current_stage: Some(initial_stage),
            accepted_result: None,
            accepted_stage: None,
            relax_failed: false,
            stage_evaluations: Vec::new(),
            failures: Vec::new(),
            rollback: None,
            procedure_gate_evidence: Vec::new(),
            science_evidence: ScottScienceEvidence::default(),
        }
    }

    pub fn record_stage_evaluation(
        &mut self,
        status: ScottStageStatus,
        result: Option<EvalResult>,
    ) {
        self.current_stage = Some(status.stage);
        self.stage_evaluations
            .push(ScottStageEvaluation { status, result });
    }

    pub fn record_failure(&mut self, failure: ScottStageFailure) {
        self.failures.push(failure);
    }

    pub fn set_accepted_result(&mut self, stage: StageIndex, result: EvalResult) {
        self.accepted_stage = Some(stage);
        self.accepted_result = Some(result);
        self.current_stage = Some(stage);
    }

    pub fn mark_relax_failed(&mut self, rollback: ScottEvalRollback) {
        self.relax_failed = true;
        self.rollback = Some(rollback);
    }

    pub fn record_validity_check(&mut self, check: ScottValidityCheck) {
        self.science_evidence.validity_checks.push(check);
    }

    pub fn set_topology_provenance(&mut self, topology: ScottTopologyProvenance) {
        self.science_evidence.topology = Some(topology);
    }

    pub fn record_procedure_gate_evidence(&mut self, evidence: ScottProcedureGateEvidence) {
        self.procedure_gate_evidence.push(evidence);
    }

    pub fn provenance(&self) -> ScottEvalProvenance {
        ScottEvalProvenance {
            completed_stages: self
                .stage_evaluations
                .iter()
                .map(|entry| entry.status.clone())
                .collect(),
            failures: self.failures.clone(),
            rollback: self.rollback.clone(),
        }
    }
}

/// Final Scott procedure outcome wrapping the concrete evaluation result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScottEvalOutcome {
    pub final_result: Option<EvalResult>,
    pub final_stage: Option<StageIndex>,
    pub relax_failed: bool,
    pub provenance: ScottEvalProvenance,
    pub state: ScottEvaluationState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScottStateDigest {
    pub request_id: String,
    pub input_label: String,
    pub current_stage: Option<StageIndex>,
    pub accepted_stage: Option<StageIndex>,
    pub accepted_label: Option<String>,
    pub stage_attempt_count: usize,
    pub failure_count: usize,
    pub procedure_gate_count: usize,
    pub procedure_rejection_count: usize,
    pub validity_failure_count: usize,
    pub relax_failed: bool,
    pub rollback_to_stage: Option<StageIndex>,
    pub last_failure_message: Option<String>,
    pub last_procedure_gate_message: Option<String>,
    pub topology_skip: bool,
    pub input_hashkey: Option<String>,
    pub final_hashkey: Option<String>,
    pub duplicate_reason: Option<ScottDuplicateReason>,
    pub duplicate_action: Option<ScottDuplicateAction>,
    pub duplicate_rank: Option<usize>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScottEvalOutcomeConsistencyError {
    #[error("final result does not match the accepted result stored in Scott evaluation state")]
    FinalResultMismatch,
    #[error("final stage does not match the accepted stage stored in Scott evaluation state")]
    FinalStageMismatch,
    #[error("relax_failed does not match the Scott evaluation state flag")]
    RelaxFailedMismatch,
    #[error("provenance does not match the Scott evaluation state projection")]
    ProvenanceMismatch,
}

/// Backend attempt payload passed into Scott normalization logic.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackendAttemptReport {
    pub stage: StageIndex,
    pub attempt: usize,
    pub backend_status: StageBackendStatus,
    pub failure_detail: Option<String>,
    pub energy: Option<f64>,
    pub gnorm: Option<f64>,
    pub relaxed_label: Option<String>,
    pub primary_output_path: Option<String>,
    pub result: Option<EvalResult>,
}

/// Scott procedure decision after inspecting a completed stage attempt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcedureDecision {
    AcceptStage,
    RetryStage,
    RejectCandidate,
    KeepPreviousAccepted,
}

impl ScottStageStatus {
    pub fn from_attempt(report: &BackendAttemptReport, convergence: NormalizedConvergence) -> Self {
        Self {
            stage: report.stage,
            attempt: report.attempt,
            backend_status: report.backend_status,
            convergence,
            energy: report.energy,
            gnorm: report.gnorm,
            relaxed_label: report.relaxed_label.clone(),
            primary_output_path: report.primary_output_path.clone(),
        }
    }
}

impl ScottEvalRollback {
    pub fn for_failed_stage(
        failed_stage: StageIndex,
        preserved_stage: Option<StageIndex>,
        final_stage_failure_policy: FinalStageFailurePolicy,
    ) -> Self {
        Self {
            failed_stage,
            rollback_to_stage: failed_stage.get().checked_sub(1).map(StageIndex),
            preserved_stage,
            mark_relax_failed: matches!(
                final_stage_failure_policy,
                FinalStageFailurePolicy::RejectCandidate
            ),
        }
    }
}

impl ScottEvalOutcome {
    pub fn validate_consistency(&self) -> Result<(), ScottEvalOutcomeConsistencyError> {
        if self.final_result != self.state.accepted_result {
            return Err(ScottEvalOutcomeConsistencyError::FinalResultMismatch);
        }
        if self.final_stage != self.state.accepted_stage {
            return Err(ScottEvalOutcomeConsistencyError::FinalStageMismatch);
        }
        if self.relax_failed != self.state.relax_failed {
            return Err(ScottEvalOutcomeConsistencyError::RelaxFailedMismatch);
        }
        if self.provenance != self.state.provenance() {
            return Err(ScottEvalOutcomeConsistencyError::ProvenanceMismatch);
        }
        Ok(())
    }

    pub fn stage_attempt_count(&self) -> usize {
        self.state.stage_evaluations.len()
    }

    pub fn failure_count(&self) -> usize {
        self.state.failures.len()
    }

    pub fn validity_failure_count(&self) -> usize {
        self.state
            .science_evidence
            .validity_checks
            .iter()
            .filter(|check| !check.passed)
            .count()
    }

    pub fn procedure_gate_count(&self) -> usize {
        self.state.procedure_gate_evidence.len()
    }

    pub fn procedure_rejection_count(&self) -> usize {
        self.state
            .procedure_gate_evidence
            .iter()
            .filter(|evidence| matches!(evidence.outcome, ScottProcedureGateOutcome::RejectStage))
            .count()
    }

    pub fn last_procedure_gate_message(&self) -> Option<&str> {
        self.state
            .procedure_gate_evidence
            .last()
            .map(|evidence| evidence.message.as_str())
    }

    pub fn rollback_to_stage(&self) -> Option<StageIndex> {
        self.state
            .rollback
            .as_ref()
            .and_then(|rollback| rollback.rollback_to_stage)
    }

    pub fn last_failure_message(&self) -> Option<&str> {
        self.state
            .failures
            .last()
            .map(|failure| failure.message.as_str())
    }

    pub fn state_digest(&self) -> ScottStateDigest {
        let duplicate = self.state.science_evidence.duplicate.as_ref();
        ScottStateDigest {
            request_id: self.state.request_id.clone(),
            input_label: self.state.input_candidate.label.clone(),
            current_stage: self.state.current_stage,
            accepted_stage: self.state.accepted_stage,
            accepted_label: self
                .state
                .accepted_result
                .as_ref()
                .map(|result| result.relaxed_candidate.label.clone()),
            stage_attempt_count: self.stage_attempt_count(),
            failure_count: self.failure_count(),
            procedure_gate_count: self.procedure_gate_count(),
            procedure_rejection_count: self.procedure_rejection_count(),
            validity_failure_count: self.validity_failure_count(),
            relax_failed: self.relax_failed,
            rollback_to_stage: self.rollback_to_stage(),
            last_failure_message: self.last_failure_message().map(ToOwned::to_owned),
            last_procedure_gate_message: self.last_procedure_gate_message().map(ToOwned::to_owned),
            topology_skip: self
                .state
                .science_evidence
                .topology
                .as_ref()
                .map(|topology| topology.topology_skip)
                .unwrap_or(false),
            input_hashkey: self
                .state
                .science_evidence
                .topology
                .as_ref()
                .and_then(|topology| topology.input_hashkey.clone()),
            final_hashkey: self
                .state
                .science_evidence
                .topology
                .as_ref()
                .and_then(|topology| topology.final_hashkey.clone()),
            duplicate_reason: duplicate.map(|duplicate| duplicate.reason),
            duplicate_action: duplicate.map(|duplicate| duplicate.action),
            duplicate_rank: duplicate.and_then(|duplicate| duplicate.matched_rank),
        }
    }
}

pub fn normalize_backend_attempt(
    report: &BackendAttemptReport,
    max_relaxation_attempts: usize,
    final_stage_failure_policy: FinalStageFailurePolicy,
    is_final_stage: bool,
) -> (
    ScottStageStatus,
    Option<ScottStageFailure>,
    ProcedureDecision,
) {
    match report.backend_status {
        StageBackendStatus::Converged | StageBackendStatus::ConvergedWithGradientWarning => {
            let status = ScottStageStatus::from_attempt(report, NormalizedConvergence::Accepted);
            (status, None, ProcedureDecision::AcceptStage)
        }
        StageBackendStatus::RequiresMoreCycles => {
            if report.attempt < max_relaxation_attempts {
                let status =
                    ScottStageStatus::from_attempt(report, NormalizedConvergence::RetryableFailure);
                let failure = ScottStageFailure {
                    stage: report.stage,
                    attempt: report.attempt,
                    backend_status: report.backend_status,
                    message: report.failure_detail.clone().unwrap_or_else(|| {
                        "backend requires more cycles; retrying current stage".into()
                    }),
                };
                (status, Some(failure), ProcedureDecision::RetryStage)
            } else {
                let status =
                    ScottStageStatus::from_attempt(report, NormalizedConvergence::FinalFailure);
                let failure = ScottStageFailure {
                    stage: report.stage,
                    attempt: report.attempt,
                    backend_status: report.backend_status,
                    message: report.failure_detail.clone().unwrap_or_else(|| {
                        "backend requires more cycles and relaxation attempts are exhausted".into()
                    }),
                };
                let decision = if is_final_stage
                    && matches!(
                        final_stage_failure_policy,
                        FinalStageFailurePolicy::KeepPreviousAccepted
                    ) {
                    ProcedureDecision::KeepPreviousAccepted
                } else {
                    ProcedureDecision::RejectCandidate
                };
                (status, Some(failure), decision)
            }
        }
        StageBackendStatus::Crashed
        | StageBackendStatus::InvalidEnergy
        | StageBackendStatus::RejectedByProcedure => {
            let status =
                ScottStageStatus::from_attempt(report, NormalizedConvergence::FinalFailure);
            let failure = ScottStageFailure {
                stage: report.stage,
                attempt: report.attempt,
                backend_status: report.backend_status,
                message: report.failure_detail.clone().unwrap_or_else(|| {
                    match report.backend_status {
                        StageBackendStatus::Crashed => {
                            "backend crashed during stage execution".into()
                        }
                        StageBackendStatus::InvalidEnergy => {
                            "backend reported an invalid energy for this stage".into()
                        }
                        StageBackendStatus::RejectedByProcedure => {
                            "stage result was rejected by Scott procedure rules".into()
                        }
                        _ => unreachable!(),
                    }
                }),
            };
            let decision = if is_final_stage
                && matches!(
                    final_stage_failure_policy,
                    FinalStageFailurePolicy::KeepPreviousAccepted
                ) {
                ProcedureDecision::KeepPreviousAccepted
            } else {
                ProcedureDecision::RejectCandidate
            };
            (status, Some(failure), decision)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_backend_attempt, BackendAttemptReport, NormalizedConvergence, ProcedureDecision,
        ScottEvalOutcome, ScottEvalOutcomeConsistencyError, ScottEvalProvenance, ScottEvalRollback,
        ScottEvaluationState, ScottStageStatus, StageBackendStatus,
    };
    use crate::{FinalStageFailurePolicy, StageIndex};
    use patina_types::{Candidate, EvalResult};
    use std::time::Duration;

    fn report(status: StageBackendStatus, attempt: usize) -> BackendAttemptReport {
        BackendAttemptReport {
            stage: StageIndex(2),
            attempt,
            backend_status: status,
            failure_detail: None,
            energy: Some(-10.0),
            gnorm: Some(1.0e-3),
            relaxed_label: Some("stage2".into()),
            primary_output_path: Some("run/gulp_klmc.gout".into()),
            result: None,
        }
    }

    #[test]
    fn converged_attempt_is_accepted() {
        let (status, failure, decision) = normalize_backend_attempt(
            &report(StageBackendStatus::Converged, 1),
            3,
            FinalStageFailurePolicy::RejectCandidate,
            false,
        );
        assert_eq!(status.convergence, NormalizedConvergence::Accepted);
        assert!(failure.is_none());
        assert_eq!(decision, ProcedureDecision::AcceptStage);
    }

    #[test]
    fn requires_more_cycles_retries_before_attempt_limit() {
        let (status, failure, decision) = normalize_backend_attempt(
            &report(StageBackendStatus::RequiresMoreCycles, 1),
            3,
            FinalStageFailurePolicy::RejectCandidate,
            false,
        );
        assert_eq!(status.convergence, NormalizedConvergence::RetryableFailure);
        assert!(failure.is_some());
        assert_eq!(decision, ProcedureDecision::RetryStage);
    }

    #[test]
    fn final_stage_failure_can_keep_previous_result() {
        let (_, failure, decision) = normalize_backend_attempt(
            &report(StageBackendStatus::Crashed, 2),
            2,
            FinalStageFailurePolicy::KeepPreviousAccepted,
            true,
        );
        assert!(failure.is_some());
        assert_eq!(decision, ProcedureDecision::KeepPreviousAccepted);
    }

    #[test]
    fn rollback_targets_previous_stage() {
        let rollback = ScottEvalRollback::for_failed_stage(
            StageIndex(3),
            Some(StageIndex(2)),
            FinalStageFailurePolicy::RejectCandidate,
        );
        assert_eq!(rollback.rollback_to_stage, Some(StageIndex(2)));
        assert_eq!(rollback.preserved_stage, Some(StageIndex(2)));
        assert!(rollback.mark_relax_failed);
    }

    fn sample_candidate() -> Candidate {
        Candidate::cluster("mg1", vec!["Mg".into()], vec![[0.0, 0.0, 0.0]])
    }

    fn sample_result() -> EvalResult {
        EvalResult {
            energy: -10.0,
            forces: vec![[0.0, 0.0, 0.0]],
            relaxed_candidate: sample_candidate(),
            converged: true,
            wall_time: Duration::from_secs(1),
        }
    }

    #[test]
    fn outcome_consistency_accepts_matching_state_projection() {
        let mut state =
            ScottEvaluationState::new("req-1".into(), sample_candidate(), StageIndex(1));
        let status = ScottStageStatus {
            stage: StageIndex(1),
            attempt: 1,
            backend_status: StageBackendStatus::Converged,
            convergence: NormalizedConvergence::Accepted,
            energy: Some(-10.0),
            gnorm: Some(1.0e-5),
            relaxed_label: Some("mg1".into()),
            primary_output_path: Some("run/gulp_klmc.gout".into()),
        };
        let result = sample_result();
        state.record_stage_evaluation(status, Some(result.clone()));
        state.set_accepted_result(StageIndex(1), result.clone());

        let outcome = ScottEvalOutcome {
            final_result: Some(result),
            final_stage: Some(StageIndex(1)),
            relax_failed: false,
            provenance: ScottEvalProvenance {
                completed_stages: state
                    .stage_evaluations
                    .iter()
                    .map(|entry| entry.status.clone())
                    .collect(),
                failures: Vec::new(),
                rollback: None,
            },
            state,
        };

        assert!(outcome.validate_consistency().is_ok());
        assert_eq!(outcome.stage_attempt_count(), 1);
        assert_eq!(outcome.failure_count(), 0);
    }

    #[test]
    fn outcome_consistency_rejects_mismatched_final_stage() {
        let state = ScottEvaluationState::new("req-1".into(), sample_candidate(), StageIndex(1));
        let outcome = ScottEvalOutcome {
            final_result: None,
            final_stage: Some(StageIndex(2)),
            relax_failed: false,
            provenance: ScottEvalProvenance::default(),
            state,
        };

        assert!(matches!(
            outcome.validate_consistency(),
            Err(ScottEvalOutcomeConsistencyError::FinalStageMismatch)
        ));
    }
}
