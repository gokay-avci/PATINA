use serde::{Deserialize, Serialize};

use patina_external::{CrystalResultMappingDecision, CrystalResultMappingStatus, CrystalRunIntent};

use crate::{
    AllocationTaskPhase, DftFailureClass, ExternalBatchReceipt, JobFailureKind,
    OrchestratedJobState, OrchestratedJobStatus, SchedulerReceiptState,
};

/// Evidence sources used when the coordinator reconciles truth after polling or restart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceSource {
    Scheduler,
    AllocationRuntime,
    Artifacts,
    Parser,
}

/// Artifact availability visible to the coordinator during polling or recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactEvidence {
    NoneObserved,
    TerminalSummaryOnly,
    RecoverableOutputs,
    TerminalSummaryAndOutputs,
}

impl ArtifactEvidence {
    pub fn has_terminal_summary(self) -> bool {
        matches!(
            self,
            Self::TerminalSummaryOnly | Self::TerminalSummaryAndOutputs
        )
    }

    pub fn has_recoverable_outputs(self) -> bool {
        matches!(
            self,
            Self::RecoverableOutputs | Self::TerminalSummaryAndOutputs
        )
    }
}

/// Scientific-validity evidence emitted after parsing or salvage inspection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParserEvidence {
    NotRun,
    CandidateResult,
    ValidResult,
    SalvageableResult,
    InvalidResult,
}

/// Program-neutral parser evidence that can be persisted without importing parser internals.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerminalParserEvidence {
    pub program: String,
    pub parser: ParserEvidence,
    pub dft_failure: Option<DftFailureClass>,
    pub run_intent: Option<String>,
    pub total_energy_hartree: Option<f64>,
    pub total_energy_ev: Option<f64>,
    pub notes: Vec<String>,
}

impl TerminalParserEvidence {
    pub fn from_crystal_result_mapping(
        program: impl Into<String>,
        decision: CrystalResultMappingDecision,
    ) -> Self {
        let (parser, dft_failure) = match decision.status {
            CrystalResultMappingStatus::CandidateForMaterialization => {
                (ParserEvidence::CandidateResult, None)
            }
            CrystalResultMappingStatus::SalvageableObservation => {
                (ParserEvidence::SalvageableResult, None)
            }
            CrystalResultMappingStatus::FailedTerminalEvidence
            | CrystalResultMappingStatus::InsufficientEvidence => (
                ParserEvidence::InvalidResult,
                Some(DftFailureClass::ParseFailed),
            ),
        };

        Self {
            program: program.into(),
            parser,
            dft_failure,
            run_intent: Some(crystal_run_intent_label(decision.run_intent).into()),
            total_energy_hartree: decision.total_energy_hartree,
            total_energy_ev: decision.total_energy_ev,
            notes: decision.notes,
        }
    }
}

fn crystal_run_intent_label(intent: CrystalRunIntent) -> &'static str {
    match intent {
        CrystalRunIntent::Unknown => "unknown",
        CrystalRunIntent::SinglePointEnergy => "single_point_energy",
        CrystalRunIntent::GeometryOptimization => "geometry_optimization",
        CrystalRunIntent::Properties => "properties",
        CrystalRunIntent::Phonon => "phonon",
        CrystalRunIntent::BandStructure => "band_structure",
        CrystalRunIntent::DensityOfStates => "density_of_states",
    }
}

/// Replay-safe operation kinds that must tolerate duplicate ingestion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplayOperationKind {
    Retrieval,
    Parsing,
    ProjectionUpdate,
}

/// Stable replay key derived from job identity, attempt, and evidence token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplayOperationKey {
    pub job_id: String,
    pub attempt: u32,
    pub operation: ReplayOperationKind,
    pub receipt_id: Option<String>,
    pub evidence_token: String,
}

impl ReplayOperationKey {
    pub fn new(
        job_id: impl Into<String>,
        attempt: u32,
        operation: ReplayOperationKind,
        receipt_id: Option<String>,
        evidence_token: impl Into<String>,
    ) -> Self {
        Self {
            job_id: job_id.into(),
            attempt,
            operation,
            receipt_id,
            evidence_token: evidence_token.into(),
        }
    }
}

/// Coordinator-visible evidence set used for reconciliation decisions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReconciliationInputs {
    pub current_status: OrchestratedJobStatus,
    pub scheduler_state: Option<SchedulerReceiptState>,
    pub allocation_phase: Option<AllocationTaskPhase>,
    pub artifacts: ArtifactEvidence,
    pub parser: ParserEvidence,
    pub dft_failure: Option<DftFailureClass>,
}

/// Result of one reconciliation pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReconciliationOutcome {
    pub authoritative_source: EvidenceSource,
    pub next_status: OrchestratedJobStatus,
    pub failure_kind: Option<JobFailureKind>,
    pub dft_failure: Option<DftFailureClass>,
    pub replay_key: Option<ReplayOperationKey>,
}

/// Persisted evidence available after coordinator restart.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecoverySnapshot {
    pub persisted_state: Option<OrchestratedJobState>,
    pub persisted_receipt: Option<ExternalBatchReceipt>,
    pub manifest_present: bool,
    pub artifacts: ArtifactEvidence,
    pub scheduler_query_state: Option<SchedulerReceiptState>,
    pub allocation_phase: Option<AllocationTaskPhase>,
    pub parser: ParserEvidence,
    pub evidence_token: String,
}

/// Reconstruction failures that prevent replay-safe reconciliation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryGap {
    MissingManifest,
    MissingPersistentState,
    MissingReceiptAndSchedulerQuery,
}

/// Builds runtime reconciliation inputs from persisted evidence after coordinator restart.
pub fn reconstruct_reconciliation_inputs(
    snapshot: &RecoverySnapshot,
) -> Result<(ReconciliationInputs, ReplayOperationKey), RecoveryGap> {
    let state = snapshot
        .persisted_state
        .as_ref()
        .ok_or(RecoveryGap::MissingPersistentState)?;
    if !snapshot.manifest_present {
        return Err(RecoveryGap::MissingManifest);
    }
    if snapshot.persisted_receipt.is_none() && snapshot.scheduler_query_state.is_none() {
        return Err(RecoveryGap::MissingReceiptAndSchedulerQuery);
    }

    let receipt_id = snapshot
        .persisted_receipt
        .as_ref()
        .map(|receipt| receipt.receipt_id.clone())
        .or_else(|| state.batch_receipt_id.clone());
    let replay_key = ReplayOperationKey::new(
        &state.job_id,
        state.attempt,
        ReplayOperationKind::ProjectionUpdate,
        receipt_id.clone(),
        snapshot.evidence_token.clone(),
    );

    Ok((
        ReconciliationInputs {
            current_status: state.status,
            scheduler_state: snapshot
                .scheduler_query_state
                .or_else(|| snapshot.persisted_receipt.as_ref().map(|r| r.state)),
            allocation_phase: snapshot.allocation_phase,
            artifacts: snapshot.artifacts,
            parser: snapshot.parser,
            dft_failure: None,
        },
        replay_key,
    ))
}

/// Resolves disagreement between scheduler, allocation-local, artifact, and parser evidence.
pub fn reconcile_job_truth(inputs: &ReconciliationInputs) -> ReconciliationOutcome {
    use AllocationTaskPhase as P;
    use ArtifactEvidence as A;
    use EvidenceSource as S;
    use OrchestratedJobStatus as O;
    use ParserEvidence as V;
    use SchedulerReceiptState as R;

    if matches!(inputs.parser, V::ValidResult) {
        return ReconciliationOutcome {
            authoritative_source: S::Parser,
            next_status: O::Completed,
            failure_kind: None,
            dft_failure: None,
            replay_key: None,
        };
    }

    if matches!(inputs.parser, V::CandidateResult) {
        return ReconciliationOutcome {
            authoritative_source: S::Parser,
            next_status: O::Parsing,
            failure_kind: None,
            dft_failure: None,
            replay_key: None,
        };
    }

    if matches!(inputs.parser, V::SalvageableResult) && inputs.artifacts.has_recoverable_outputs() {
        return ReconciliationOutcome {
            authoritative_source: S::Parser,
            next_status: O::Completed,
            failure_kind: None,
            dft_failure: None,
            replay_key: None,
        };
    }

    if let Some(dft_failure) = inputs.dft_failure {
        let next_status = if dft_failure == DftFailureClass::Cancelled {
            O::Cancelled
        } else {
            O::Failed
        };
        return ReconciliationOutcome {
            authoritative_source: match dft_failure {
                DftFailureClass::ParseFailed => S::Parser,
                DftFailureClass::RetrievalFailed | DftFailureClass::MissingTerminalOutput => {
                    S::Artifacts
                }
                DftFailureClass::BackendRuntimeFailed => S::AllocationRuntime,
                DftFailureClass::QueuedTooLong
                | DftFailureClass::TimedOut
                | DftFailureClass::Cancelled
                | DftFailureClass::LostSchedulerReceipt => S::Scheduler,
            },
            next_status,
            failure_kind: Some(dft_failure.job_failure_kind()),
            dft_failure: Some(dft_failure),
            replay_key: None,
        };
    }

    if matches!(inputs.parser, V::InvalidResult) {
        return ReconciliationOutcome {
            authoritative_source: S::Parser,
            next_status: O::Failed,
            failure_kind: Some(JobFailureKind::Parse),
            dft_failure: Some(DftFailureClass::ParseFailed),
            replay_key: None,
        };
    }

    if matches!(inputs.scheduler_state, Some(R::Cancelled)) {
        return ReconciliationOutcome {
            authoritative_source: S::Scheduler,
            next_status: O::Cancelled,
            failure_kind: Some(JobFailureKind::CancelledByOperator),
            dft_failure: Some(DftFailureClass::Cancelled),
            replay_key: None,
        };
    }

    if matches!(inputs.scheduler_state, Some(R::Lost)) {
        return ReconciliationOutcome {
            authoritative_source: S::Scheduler,
            next_status: O::Failed,
            failure_kind: Some(JobFailureKind::LostReceipt),
            dft_failure: Some(DftFailureClass::LostSchedulerReceipt),
            replay_key: None,
        };
    }

    if matches!(inputs.allocation_phase, Some(P::Failed))
        && matches!(
            inputs.scheduler_state,
            Some(R::Running) | Some(R::Completed) | Some(R::Queued) | None
        )
    {
        return ReconciliationOutcome {
            authoritative_source: S::AllocationRuntime,
            next_status: O::Failed,
            failure_kind: Some(JobFailureKind::Runtime),
            dft_failure: Some(DftFailureClass::BackendRuntimeFailed),
            replay_key: None,
        };
    }

    if matches!(inputs.scheduler_state, Some(R::Failed))
        && inputs.artifacts.has_recoverable_outputs()
    {
        return ReconciliationOutcome {
            authoritative_source: S::Artifacts,
            next_status: O::Retrieving,
            failure_kind: None,
            dft_failure: None,
            replay_key: None,
        };
    }

    if matches!(inputs.scheduler_state, Some(R::Failed)) {
        return ReconciliationOutcome {
            authoritative_source: S::Scheduler,
            next_status: O::Failed,
            failure_kind: Some(JobFailureKind::Runtime),
            dft_failure: Some(DftFailureClass::BackendRuntimeFailed),
            replay_key: None,
        };
    }

    if matches!(inputs.scheduler_state, Some(R::Completed))
        && inputs.artifacts.has_recoverable_outputs()
        && !matches!(inputs.parser, V::InvalidResult)
    {
        return ReconciliationOutcome {
            authoritative_source: S::Artifacts,
            next_status: O::Retrieving,
            failure_kind: None,
            dft_failure: None,
            replay_key: None,
        };
    }

    if matches!(inputs.scheduler_state, Some(R::Completed)) && inputs.artifacts == A::NoneObserved {
        return ReconciliationOutcome {
            authoritative_source: S::Scheduler,
            next_status: O::Failed,
            failure_kind: Some(JobFailureKind::LostReceipt),
            dft_failure: Some(DftFailureClass::MissingTerminalOutput),
            replay_key: None,
        };
    }

    if matches!(inputs.scheduler_state, Some(R::Queued) | Some(R::Submitted)) {
        return ReconciliationOutcome {
            authoritative_source: S::Scheduler,
            next_status: O::QueuedScheduler,
            failure_kind: None,
            dft_failure: None,
            replay_key: None,
        };
    }

    if matches!(inputs.scheduler_state, Some(R::Running)) {
        let next_status = match inputs.allocation_phase {
            Some(P::Materializing) | Some(P::LaunchingBackend) => O::Materializing,
            Some(P::BackendRunning) => O::RunningExternal,
            Some(P::Retrieving) => O::Retrieving,
            Some(P::Parsing) => O::Parsing,
            Some(P::Completed) => O::Retrieving,
            Some(P::Failed) | None => O::RunningExternal,
        };
        return ReconciliationOutcome {
            authoritative_source: S::AllocationRuntime,
            next_status,
            failure_kind: None,
            dft_failure: None,
            replay_key: None,
        };
    }

    ReconciliationOutcome {
        authoritative_source: S::Scheduler,
        next_status: inputs.current_status,
        failure_kind: None,
        dft_failure: None,
        replay_key: None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        reconcile_job_truth, reconstruct_reconciliation_inputs, ArtifactEvidence, EvidenceSource,
        ParserEvidence, ReconciliationInputs, RecoveryGap, RecoverySnapshot, ReplayOperationKey,
        ReplayOperationKind, TerminalParserEvidence,
    };
    use crate::{
        AllocationTaskPhase, DftFailureClass, ExternalBatchReceipt, JobFailureKind,
        OrchestratedJobState, OrchestratedJobStatus, SchedulerFamily, SchedulerJobIdentifier,
        SchedulerReceiptState,
    };
    use patina_external::{
        CrystalResultMappingDecision, CrystalResultMappingStatus, CrystalRunIntent,
    };

    fn sample_state() -> OrchestratedJobState {
        let mut state = OrchestratedJobState::new("job-1", 10);
        state.attempt = 2;
        state.batch_receipt_id = Some("slurm:123:job-1".into());
        state.status = OrchestratedJobStatus::RunningExternal;
        state
    }

    fn sample_receipt() -> ExternalBatchReceipt {
        ExternalBatchReceipt {
            receipt_id: "slurm:123:job-1".into(),
            job_id: "job-1".into(),
            scheduler_job: SchedulerJobIdentifier {
                scheduler_family: SchedulerFamily::Slurm,
                allocation_id: "123".into(),
                step_id: None,
                array_job_id: None,
                array_index: None,
            },
            state: SchedulerReceiptState::Running,
            submitted_at_ms: 1,
            last_observed_at_ms: 2,
            launch_host: None,
            workdir: None,
            launcher_provenance: None,
            last_telemetry: None,
        }
    }

    #[test]
    fn parser_validity_wins_over_other_sources() {
        let outcome = reconcile_job_truth(&ReconciliationInputs {
            current_status: OrchestratedJobStatus::Parsing,
            scheduler_state: Some(SchedulerReceiptState::Failed),
            allocation_phase: Some(AllocationTaskPhase::Failed),
            artifacts: ArtifactEvidence::TerminalSummaryAndOutputs,
            parser: ParserEvidence::ValidResult,
            dft_failure: Some(DftFailureClass::BackendRuntimeFailed),
        });

        assert_eq!(outcome.authoritative_source, EvidenceSource::Parser);
        assert_eq!(outcome.next_status, OrchestratedJobStatus::Completed);
    }

    #[test]
    fn parser_candidate_result_moves_to_materialization_without_completion() {
        let outcome = reconcile_job_truth(&ReconciliationInputs {
            current_status: OrchestratedJobStatus::Parsing,
            scheduler_state: Some(SchedulerReceiptState::Completed),
            allocation_phase: Some(AllocationTaskPhase::Completed),
            artifacts: ArtifactEvidence::TerminalSummaryAndOutputs,
            parser: ParserEvidence::CandidateResult,
            dft_failure: None,
        });

        assert_eq!(outcome.authoritative_source, EvidenceSource::Parser);
        assert_eq!(outcome.next_status, OrchestratedJobStatus::Parsing);
        assert_eq!(outcome.failure_kind, None);
    }

    #[test]
    fn crystal_result_mapping_projects_to_portable_terminal_parser_evidence() {
        let record = TerminalParserEvidence::from_crystal_result_mapping(
            "crystal",
            CrystalResultMappingDecision {
                status: CrystalResultMappingStatus::CandidateForMaterialization,
                run_intent: CrystalRunIntent::GeometryOptimization,
                total_energy_hartree: Some(-75.0),
                total_energy_ev: Some(-2040.8539684491),
                notes: vec!["fixture validation pending".into()],
            },
        );

        assert_eq!(record.program, "crystal");
        assert_eq!(record.parser, ParserEvidence::CandidateResult);
        assert_eq!(record.dft_failure, None);
        assert_eq!(record.run_intent.as_deref(), Some("geometry_optimization"));
        assert_eq!(record.total_energy_hartree, Some(-75.0));
    }

    #[test]
    fn allocation_runtime_failure_beats_running_scheduler() {
        let outcome = reconcile_job_truth(&ReconciliationInputs {
            current_status: OrchestratedJobStatus::RunningExternal,
            scheduler_state: Some(SchedulerReceiptState::Running),
            allocation_phase: Some(AllocationTaskPhase::Failed),
            artifacts: ArtifactEvidence::NoneObserved,
            parser: ParserEvidence::NotRun,
            dft_failure: None,
        });

        assert_eq!(
            outcome.authoritative_source,
            EvidenceSource::AllocationRuntime
        );
        assert_eq!(outcome.next_status, OrchestratedJobStatus::Failed);
    }

    #[test]
    fn failed_scheduler_with_outputs_moves_to_retrieval() {
        let outcome = reconcile_job_truth(&ReconciliationInputs {
            current_status: OrchestratedJobStatus::RunningExternal,
            scheduler_state: Some(SchedulerReceiptState::Failed),
            allocation_phase: None,
            artifacts: ArtifactEvidence::RecoverableOutputs,
            parser: ParserEvidence::NotRun,
            dft_failure: None,
        });

        assert_eq!(outcome.authoritative_source, EvidenceSource::Artifacts);
        assert_eq!(outcome.next_status, OrchestratedJobStatus::Retrieving);
    }

    #[test]
    fn explicit_retrieval_failure_becomes_typed_dft_failure() {
        let outcome = reconcile_job_truth(&ReconciliationInputs {
            current_status: OrchestratedJobStatus::Retrieving,
            scheduler_state: Some(SchedulerReceiptState::Completed),
            allocation_phase: Some(AllocationTaskPhase::Retrieving),
            artifacts: ArtifactEvidence::RecoverableOutputs,
            parser: ParserEvidence::NotRun,
            dft_failure: Some(DftFailureClass::RetrievalFailed),
        });

        assert_eq!(outcome.authoritative_source, EvidenceSource::Artifacts);
        assert_eq!(outcome.next_status, OrchestratedJobStatus::Failed);
        assert_eq!(outcome.failure_kind, Some(JobFailureKind::Retrieval));
        assert_eq!(outcome.dft_failure, Some(DftFailureClass::RetrievalFailed));
    }

    #[test]
    fn explicit_parse_failure_becomes_typed_dft_failure() {
        let outcome = reconcile_job_truth(&ReconciliationInputs {
            current_status: OrchestratedJobStatus::Parsing,
            scheduler_state: Some(SchedulerReceiptState::Completed),
            allocation_phase: Some(AllocationTaskPhase::Parsing),
            artifacts: ArtifactEvidence::TerminalSummaryAndOutputs,
            parser: ParserEvidence::InvalidResult,
            dft_failure: Some(DftFailureClass::ParseFailed),
        });

        assert_eq!(outcome.authoritative_source, EvidenceSource::Parser);
        assert_eq!(outcome.next_status, OrchestratedJobStatus::Failed);
        assert_eq!(outcome.failure_kind, Some(JobFailureKind::Parse));
        assert_eq!(outcome.dft_failure, Some(DftFailureClass::ParseFailed));
    }

    #[test]
    fn explicit_queue_timeout_uses_scheduler_source() {
        let outcome = reconcile_job_truth(&ReconciliationInputs {
            current_status: OrchestratedJobStatus::QueuedScheduler,
            scheduler_state: Some(SchedulerReceiptState::Queued),
            allocation_phase: None,
            artifacts: ArtifactEvidence::NoneObserved,
            parser: ParserEvidence::NotRun,
            dft_failure: Some(DftFailureClass::QueuedTooLong),
        });

        assert_eq!(outcome.authoritative_source, EvidenceSource::Scheduler);
        assert_eq!(outcome.next_status, OrchestratedJobStatus::Failed);
        assert_eq!(outcome.failure_kind, Some(JobFailureKind::QueueTimeout));
        assert_eq!(outcome.dft_failure, Some(DftFailureClass::QueuedTooLong));
    }

    #[test]
    fn completed_scheduler_without_outputs_is_missing_terminal_output() {
        let outcome = reconcile_job_truth(&ReconciliationInputs {
            current_status: OrchestratedJobStatus::RunningExternal,
            scheduler_state: Some(SchedulerReceiptState::Completed),
            allocation_phase: Some(AllocationTaskPhase::Completed),
            artifacts: ArtifactEvidence::NoneObserved,
            parser: ParserEvidence::NotRun,
            dft_failure: None,
        });

        assert_eq!(outcome.authoritative_source, EvidenceSource::Scheduler);
        assert_eq!(outcome.next_status, OrchestratedJobStatus::Failed);
        assert_eq!(outcome.failure_kind, Some(JobFailureKind::LostReceipt));
        assert_eq!(
            outcome.dft_failure,
            Some(DftFailureClass::MissingTerminalOutput)
        );
    }

    #[test]
    fn recovery_reconstruction_requires_manifest_and_state() {
        let err = reconstruct_reconciliation_inputs(&RecoverySnapshot {
            persisted_state: None,
            persisted_receipt: None,
            manifest_present: false,
            artifacts: ArtifactEvidence::NoneObserved,
            scheduler_query_state: None,
            allocation_phase: None,
            parser: ParserEvidence::NotRun,
            evidence_token: "snap-1".into(),
        })
        .expect_err("missing state should fail");

        assert_eq!(err, RecoveryGap::MissingPersistentState);
    }

    #[test]
    fn recovery_reconstruction_derives_projection_update_key() {
        let (inputs, replay_key) = reconstruct_reconciliation_inputs(&RecoverySnapshot {
            persisted_state: Some(sample_state()),
            persisted_receipt: Some(sample_receipt()),
            manifest_present: true,
            artifacts: ArtifactEvidence::TerminalSummaryAndOutputs,
            scheduler_query_state: Some(SchedulerReceiptState::Completed),
            allocation_phase: Some(AllocationTaskPhase::Retrieving),
            parser: ParserEvidence::NotRun,
            evidence_token: "terminal-summary-sha256".into(),
        })
        .expect("reconstruct");

        assert_eq!(
            inputs.scheduler_state,
            Some(SchedulerReceiptState::Completed)
        );
        assert_eq!(
            inputs.artifacts,
            ArtifactEvidence::TerminalSummaryAndOutputs
        );
        assert_eq!(
            replay_key,
            ReplayOperationKey::new(
                "job-1",
                2,
                ReplayOperationKind::ProjectionUpdate,
                Some("slurm:123:job-1".into()),
                "terminal-summary-sha256"
            )
        );
    }
}
