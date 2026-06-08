use patina_external::{ExternalEvaluationOutcome, ExternalEvaluationStatus};
use patina_runtime::{
    external_stage_outcome_to_runtime_report_with_context, PriorAcceptedStageState,
    RuntimeDiagnostics, ScottRuntimeJob, ScottRuntimeReport, ScottRuntimeStatus,
    StageJobCompletionContext, StageJobCompletionError,
};
use thiserror::Error;

use crate::{
    reconcile_job_truth, AllocationTaskPhase, AllocationTerminalSummary, ArtifactEvidence,
    DftFailureClass, EvidenceSource, ExternalBatchReceipt, JobFailureKind, OrchestratedJobState,
    OrchestratedJobStatus, ParserEvidence, ReconciliationInputs, ReplayOperationKey,
    ReplayOperationKind, SchedulerReceiptState,
};

/// Context used when finalizing one stage job from scheduler plus allocation-local evidence.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ReconciledStageCompletionContext {
    pub scheduler_receipt: Option<ExternalBatchReceipt>,
    pub allocation_summary: Option<AllocationTerminalSummary>,
    pub orchestration_state: Option<OrchestratedJobState>,
    pub prior_accepted: Option<PriorAcceptedStageState>,
}

/// Errors returned while reconciling terminal allocation evidence into a Scott report.
#[derive(Debug, Error)]
pub enum ReconciledStageCompletionError {
    #[error(
        "reconciliation resolved to non-terminal Scott status `{status:?}` for job `{job_id}`"
    )]
    NonTerminalReconciliation {
        job_id: String,
        status: OrchestratedJobStatus,
    },
    #[error("stage completion bridge failed: {0}")]
    StageCompletion(#[from] StageJobCompletionError),
}

/// Integrates scheduler receipt, allocation terminal summary, and terminal external outcome.
///
/// This is the first point where `patina-ulab` wires scheduler/container evidence into the Scott
/// stage completion bridge. If reconciliation still resolves to a non-terminal state, the caller
/// must continue polling instead of fabricating completion.
pub fn reconciled_stage_completion_to_runtime_report(
    job: &ScottRuntimeJob,
    outcome: ExternalEvaluationOutcome,
    context: ReconciledStageCompletionContext,
) -> Result<ScottRuntimeReport, ReconciledStageCompletionError> {
    let current_status = context
        .orchestration_state
        .as_ref()
        .map(|state| state.status)
        .unwrap_or(OrchestratedJobStatus::RunningExternal);
    let scheduler_state = context
        .scheduler_receipt
        .as_ref()
        .map(|receipt| receipt.state);
    let allocation_phase = context
        .allocation_summary
        .as_ref()
        .map(|summary| summary.final_phase);
    let artifacts = derive_artifact_evidence(context.allocation_summary.as_ref(), &outcome);
    let parser = derive_parser_evidence(&outcome);

    let reconciliation = reconcile_job_truth(&ReconciliationInputs {
        current_status,
        scheduler_state,
        allocation_phase,
        artifacts,
        parser,
        dft_failure: derive_dft_failure(&outcome),
    });

    let diagnostics = build_reconciled_diagnostics(
        context.scheduler_receipt.as_ref(),
        context.allocation_summary.as_ref(),
        context.orchestration_state.as_ref(),
        &reconciliation,
        artifacts,
        parser,
    );

    match reconciliation.next_status {
        OrchestratedJobStatus::Completed
        | OrchestratedJobStatus::Retrieving
        | OrchestratedJobStatus::Parsing => external_stage_outcome_to_runtime_report_with_context(
            job,
            outcome,
            StageJobCompletionContext {
                diagnostics,
                prior_accepted: context.prior_accepted,
            },
        )
        .map_err(Into::into),
        OrchestratedJobStatus::Failed => Ok(ScottRuntimeReport {
            identity: job.identity.clone(),
            status: ScottRuntimeStatus::Failed,
            stage_result: None,
            generated_jobs: Vec::new(),
            diagnostics,
        }),
        OrchestratedJobStatus::Cancelled => Ok(ScottRuntimeReport {
            identity: job.identity.clone(),
            status: ScottRuntimeStatus::Cancelled,
            stage_result: None,
            generated_jobs: Vec::new(),
            diagnostics,
        }),
        other => Err(ReconciledStageCompletionError::NonTerminalReconciliation {
            job_id: job.identity.job_id.clone(),
            status: other,
        }),
    }
}

fn derive_parser_evidence(outcome: &ExternalEvaluationOutcome) -> ParserEvidence {
    match (&outcome.status, &outcome.result) {
        (ExternalEvaluationStatus::Converged, Some(_)) => ParserEvidence::ValidResult,
        (ExternalEvaluationStatus::NotConverged, Some(_)) => ParserEvidence::SalvageableResult,
        (ExternalEvaluationStatus::Converged, None)
        | (ExternalEvaluationStatus::NotConverged, None) => ParserEvidence::InvalidResult,
        (ExternalEvaluationStatus::Submitted, _) | (ExternalEvaluationStatus::ExportedOnly, _) => {
            ParserEvidence::NotRun
        }
    }
}

fn derive_dft_failure(outcome: &ExternalEvaluationOutcome) -> Option<DftFailureClass> {
    match (&outcome.status, &outcome.result) {
        (ExternalEvaluationStatus::Converged, None)
        | (ExternalEvaluationStatus::NotConverged, None) => Some(DftFailureClass::ParseFailed),
        _ => None,
    }
}

fn derive_artifact_evidence(
    summary: Option<&AllocationTerminalSummary>,
    outcome: &ExternalEvaluationOutcome,
) -> ArtifactEvidence {
    let has_summary = summary.is_some();
    let has_outputs = !outcome.artifacts.output_paths.is_empty()
        || outcome.artifacts.stdout_path.is_some()
        || summary
            .map(|summary| !summary.artifact_roots.is_empty())
            .unwrap_or(false);

    match (has_summary, has_outputs) {
        (false, false) => ArtifactEvidence::NoneObserved,
        (true, false) => ArtifactEvidence::TerminalSummaryOnly,
        (false, true) => ArtifactEvidence::RecoverableOutputs,
        (true, true) => ArtifactEvidence::TerminalSummaryAndOutputs,
    }
}

fn build_reconciled_diagnostics(
    receipt: Option<&ExternalBatchReceipt>,
    summary: Option<&AllocationTerminalSummary>,
    state: Option<&OrchestratedJobState>,
    reconciliation: &crate::ReconciliationOutcome,
    artifacts: ArtifactEvidence,
    parser: ParserEvidence,
) -> RuntimeDiagnostics {
    let mut diagnostics = RuntimeDiagnostics::default();

    if let Some(receipt) = receipt {
        diagnostics.workdir = receipt.workdir.as_ref().map(Into::into);
        if let Some(provenance) = &receipt.launcher_provenance {
            diagnostics.launch_provenance = Some(patina_runtime::RuntimeLaunchProvenance {
                site_name: Some(provenance.site_name.clone()),
                scheduler_family: Some(
                    match provenance.scheduler_family {
                        crate::SchedulerFamily::Local => "local",
                        crate::SchedulerFamily::Slurm => "slurm",
                        crate::SchedulerFamily::Pbs => "pbs",
                        crate::SchedulerFamily::GridEngine => "grid_engine",
                        crate::SchedulerFamily::Unknown => "unknown",
                    }
                    .into(),
                ),
                launch_strategy: Some(
                    match provenance.launch_strategy {
                        crate::LaunchStrategy::Direct => "direct",
                        crate::LaunchStrategy::Srun => "srun",
                        crate::LaunchStrategy::Mpirun => "mpirun",
                        crate::LaunchStrategy::PbsMpi => "pbs_mpi",
                    }
                    .into(),
                ),
                launch_command: Some(provenance.launch_command.clone()),
                submit_command: provenance.submit_command.clone(),
                status_command: provenance.status_command.clone(),
                accounting_command: provenance.accounting_command.clone(),
                cancel_command: provenance.cancel_command.clone(),
                telemetry_command: provenance.telemetry_command.clone(),
                scheduler_job_id_env: provenance.scheduler_job_id_env.clone(),
                module_environment: Some(
                    match provenance.module_environment {
                        crate::ModuleEnvironment::None => "none",
                        crate::ModuleEnvironment::EnvironmentModules => "environment_modules",
                        crate::ModuleEnvironment::Lmod => "lmod",
                    }
                    .into(),
                ),
                scratch_mode: Some(
                    match provenance.scratch_mode {
                        crate::ScratchMode::DurableOnly => "durable_only",
                        crate::ScratchMode::SharedFilesystem => "shared_filesystem",
                        crate::ScratchMode::NodeLocal => "node_local",
                    }
                    .into(),
                ),
                preferred_submission_mode: Some(
                    match provenance.preferred_submission_mode {
                        crate::SubmissionMode::DirectRuntime => "direct_runtime",
                        crate::SubmissionMode::WithinAllocation => "within_allocation",
                        crate::SubmissionMode::JobArray => "job_array",
                        crate::SubmissionMode::NestedBatch => "nested_batch",
                    }
                    .into(),
                ),
                poll_interval_secs: Some(provenance.poll_interval_secs),
            });
            diagnostics
                .metrics
                .insert("scheduler_receipt_id".into(), receipt.receipt_id.clone());
            diagnostics.metrics.insert(
                "scheduler_state".into(),
                scheduler_state_label(receipt.state).into(),
            );
        }
    }

    if let Some(summary) = summary {
        diagnostics.metrics.insert(
            "allocation_terminal_phase".into(),
            allocation_phase_label(summary.final_phase).into(),
        );
        diagnostics.metrics.insert(
            "allocation_observed_at_ms".into(),
            summary.observed_at_ms.to_string(),
        );
        if let Some(shard_id) = &summary.shard_id {
            diagnostics
                .metrics
                .insert("allocation_shard_id".into(), shard_id.clone());
        }
    }

    diagnostics.metrics.insert(
        "reconciliation_authoritative_source".into(),
        evidence_source_label(reconciliation.authoritative_source).into(),
    );
    diagnostics.metrics.insert(
        "reconciliation_next_status".into(),
        orchestrated_status_label(reconciliation.next_status).into(),
    );
    diagnostics.metrics.insert(
        "artifact_evidence".into(),
        artifact_evidence_label(artifacts).into(),
    );
    diagnostics.metrics.insert(
        "parser_evidence".into(),
        parser_evidence_label(parser).into(),
    );

    if let Some(failure_kind) = reconciliation.failure_kind {
        diagnostics.metrics.insert(
            "reconciliation_failure_kind".into(),
            failure_kind_label(failure_kind).into(),
        );
    }

    if let Some(dft_failure) = reconciliation.dft_failure {
        diagnostics.metrics.insert(
            "dft_failure_class".into(),
            dft_failure_class_label(dft_failure).into(),
        );
    }

    if let Some(state) = state {
        let replay_key = ReplayOperationKey::new(
            &state.job_id,
            state.attempt,
            ReplayOperationKind::ProjectionUpdate,
            receipt.map(|entry| entry.receipt_id.clone()),
            evidence_token(receipt, summary, state),
        );
        diagnostics.metrics.insert(
            "replay_projection_update_key".into(),
            format!(
                "{}:{}:{}:{}",
                replay_key.job_id,
                replay_key.attempt,
                replay_operation_label(replay_key.operation),
                replay_key.evidence_token
            ),
        );
    }

    diagnostics.message = Some(format!(
        "reconciled terminal evidence from `{}` into `{}`",
        evidence_source_label(reconciliation.authoritative_source),
        orchestrated_status_label(reconciliation.next_status)
    ));

    diagnostics
}

fn evidence_token(
    receipt: Option<&ExternalBatchReceipt>,
    summary: Option<&AllocationTerminalSummary>,
    state: &OrchestratedJobState,
) -> String {
    if let Some(summary) = summary {
        return format!(
            "{}:{}:{}",
            summary.receipt_id,
            summary.observed_at_ms,
            allocation_phase_label(summary.final_phase)
        );
    }

    if let Some(receipt) = receipt {
        return format!(
            "{}:{}:{}",
            receipt.receipt_id,
            receipt.last_observed_at_ms,
            scheduler_state_label(receipt.state)
        );
    }

    format!("{}:{}:no-receipt", state.job_id, state.attempt)
}

fn evidence_source_label(source: EvidenceSource) -> &'static str {
    match source {
        EvidenceSource::Scheduler => "scheduler",
        EvidenceSource::AllocationRuntime => "allocation_runtime",
        EvidenceSource::Artifacts => "artifacts",
        EvidenceSource::Parser => "parser",
    }
}

fn artifact_evidence_label(evidence: ArtifactEvidence) -> &'static str {
    match evidence {
        ArtifactEvidence::NoneObserved => "none_observed",
        ArtifactEvidence::TerminalSummaryOnly => "terminal_summary_only",
        ArtifactEvidence::RecoverableOutputs => "recoverable_outputs",
        ArtifactEvidence::TerminalSummaryAndOutputs => "terminal_summary_and_outputs",
    }
}

fn parser_evidence_label(evidence: ParserEvidence) -> &'static str {
    match evidence {
        ParserEvidence::NotRun => "not_run",
        ParserEvidence::CandidateResult => "candidate_result",
        ParserEvidence::ValidResult => "valid_result",
        ParserEvidence::SalvageableResult => "salvageable_result",
        ParserEvidence::InvalidResult => "invalid_result",
    }
}

fn orchestrated_status_label(status: OrchestratedJobStatus) -> &'static str {
    match status {
        OrchestratedJobStatus::Pending => "pending",
        OrchestratedJobStatus::Admitted => "admitted",
        OrchestratedJobStatus::Leased => "leased",
        OrchestratedJobStatus::Materializing => "materializing",
        OrchestratedJobStatus::Staged => "staged",
        OrchestratedJobStatus::SubmittedScheduler => "submitted_scheduler",
        OrchestratedJobStatus::QueuedScheduler => "queued_scheduler",
        OrchestratedJobStatus::RunningExternal => "running_external",
        OrchestratedJobStatus::Retrieving => "retrieving",
        OrchestratedJobStatus::Parsing => "parsing",
        OrchestratedJobStatus::Completed => "completed",
        OrchestratedJobStatus::Failed => "failed",
        OrchestratedJobStatus::Cancelled => "cancelled",
        OrchestratedJobStatus::Reclaimed => "reclaimed",
    }
}

fn scheduler_state_label(state: SchedulerReceiptState) -> &'static str {
    match state {
        SchedulerReceiptState::Submitted => "submitted",
        SchedulerReceiptState::Queued => "queued",
        SchedulerReceiptState::Running => "running",
        SchedulerReceiptState::Completed => "completed",
        SchedulerReceiptState::Failed => "failed",
        SchedulerReceiptState::Cancelled => "cancelled",
        SchedulerReceiptState::Lost => "lost",
    }
}

fn allocation_phase_label(phase: AllocationTaskPhase) -> &'static str {
    match phase {
        AllocationTaskPhase::Materializing => "materializing",
        AllocationTaskPhase::LaunchingBackend => "launching_backend",
        AllocationTaskPhase::BackendRunning => "backend_running",
        AllocationTaskPhase::Retrieving => "retrieving",
        AllocationTaskPhase::Parsing => "parsing",
        AllocationTaskPhase::Completed => "completed",
        AllocationTaskPhase::Failed => "failed",
    }
}

fn failure_kind_label(kind: JobFailureKind) -> &'static str {
    match kind {
        JobFailureKind::Preflight => "preflight",
        JobFailureKind::Launch => "launch",
        JobFailureKind::QueueTimeout => "queue_timeout",
        JobFailureKind::Runtime => "runtime",
        JobFailureKind::Retrieval => "retrieval",
        JobFailureKind::Parse => "parse",
        JobFailureKind::Timeout => "timeout",
        JobFailureKind::LostWorker => "lost_worker",
        JobFailureKind::LostReceipt => "lost_receipt",
        JobFailureKind::CancelledByOperator => "cancelled_by_operator",
    }
}

fn dft_failure_class_label(class: DftFailureClass) -> &'static str {
    match class {
        DftFailureClass::QueuedTooLong => "queued_too_long",
        DftFailureClass::TimedOut => "timed_out",
        DftFailureClass::Cancelled => "cancelled",
        DftFailureClass::RetrievalFailed => "retrieval_failed",
        DftFailureClass::ParseFailed => "parse_failed",
        DftFailureClass::MissingTerminalOutput => "missing_terminal_output",
        DftFailureClass::BackendRuntimeFailed => "backend_runtime_failed",
        DftFailureClass::LostSchedulerReceipt => "lost_scheduler_receipt",
    }
}

fn replay_operation_label(operation: ReplayOperationKind) -> &'static str {
    match operation {
        ReplayOperationKind::Retrieval => "retrieval",
        ReplayOperationKind::Parsing => "parsing",
        ReplayOperationKind::ProjectionUpdate => "projection_update",
    }
}

#[cfg(test)]
mod tests {
    use super::{reconciled_stage_completion_to_runtime_report, ReconciledStageCompletionContext};
    use crate::{
        AllocationTaskPhase, AllocationTerminalSummary, ExternalBatchReceipt, OrchestratedJobState,
        OrchestratedJobStatus, SchedulerFamily, SchedulerJobIdentifier, SchedulerReceiptState,
    };
    use indexmap::IndexMap;
    use patina_evaluator::{
        FinalStageFailurePolicy, ProcedureAction, ProcedureCursor, ScottBackendMode,
        ScottEvaluatorPlan, ScottEvaluatorSettings, ScottLatticeMode, ScottProcedureIntent,
        ScottProcedurePlan, ScottProcedureRequest, StageEngine, StageIndex, StagePlan,
        StageSelection, StageTicket,
    };
    use patina_external::{ExternalArtifacts, ExternalEvaluationOutcome, ExternalEvaluationStatus};
    use patina_runtime::{
        ScottJobIdentity, ScottJobKind, ScottJobRole, ScottRuntimeJob, ScottRuntimeStatus,
        ScottStageDispatch,
    };
    use patina_types::{Candidate, EvalResult};
    use std::time::Duration;

    fn candidate() -> Candidate {
        Candidate::cluster("mg", vec!["Mg".into()], vec![[0.0, 0.0, 0.0]])
    }

    fn stage_job(job_id: &str) -> ScottRuntimeJob {
        let ticket = StageTicket {
            request_id: format!("req-{job_id}"),
            backend_mode: ScottBackendMode::Gulp,
            stage: StageIndex(1),
            attempt: 1,
            workdir: format!("/scratch/{job_id}").into(),
        };
        ScottRuntimeJob {
            identity: ScottJobIdentity {
                job_id: job_id.into(),
                parent_job_id: Some("controller-1".into()),
                candidate_label: Some("mg".into()),
            },
            role: ScottJobRole::Worker,
            kind: ScottJobKind::EvaluateStage {
                dispatch: Box::new(ScottStageDispatch {
                    plan: ScottProcedurePlan {
                        evaluator: ScottEvaluatorPlan {
                            master_template: "/tmp/Master.gin".into(),
                            atoms_in: None,
                            work_root: "/tmp".into(),
                            settings: ScottEvaluatorSettings {
                                backend_mode: ScottBackendMode::Gulp,
                                procedure_intent: ScottProcedureIntent::ProductionRun,
                                lattice_mode: ScottLatticeMode::Cluster,
                                max_relaxation_attempts: 2,
                                gnorm_tolerance: 1e-4,
                                final_stage_failure_policy:
                                    FinalStageFailurePolicy::RejectCandidate,
                                retrieve_relaxed_geometry: true,
                                keep_stage_artifacts: true,
                            },
                        },
                        stages: StageSelection {
                            stages: vec![StagePlan {
                                stage: StageIndex(1),
                                engine: StageEngine::Gulp,
                                refine_if_energy_below: None,
                                energy_min_threshold: None,
                                energy_max_threshold: None,
                                keep_only_if_final_stage: true,
                            }],
                        },
                    },
                    request: ScottProcedureRequest {
                        candidate: candidate(),
                        request_id: format!("req-{job_id}"),
                        workdir: format!("/tmp/{job_id}").into(),
                    },
                    cursor: ProcedureCursor::new(ticket.clone()),
                    action: ProcedureAction::Submit(ticket),
                }),
            },
            requested_cores: 1,
            requested_gpus: 0,
            required_tags: Vec::new(),
            labels: IndexMap::new(),
        }
    }

    fn receipt(state: SchedulerReceiptState) -> ExternalBatchReceipt {
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
            state,
            submitted_at_ms: 1,
            last_observed_at_ms: 2,
            launch_host: Some("login01".into()),
            workdir: Some("/scratch/job-1/stage".into()),
            launcher_provenance: Some(crate::SiteProfile::archer2().launcher_provenance()),
            last_telemetry: None,
        }
    }

    fn summary(phase: AllocationTaskPhase) -> AllocationTerminalSummary {
        AllocationTerminalSummary {
            receipt_id: "slurm:123:job-1".into(),
            shard_id: None,
            observed_at_ms: 99,
            final_phase: phase,
            completed_job_ids: vec!["job-1".into()],
            failed_job_ids: Vec::new(),
            salvageable_job_ids: Vec::new(),
            artifact_roots: vec!["/scratch/job-1/stage_01_attempt_01".into()],
            message: Some("terminal summary".into()),
        }
    }

    fn converged_outcome() -> ExternalEvaluationOutcome {
        ExternalEvaluationOutcome {
            status: ExternalEvaluationStatus::Converged,
            result: Some(EvalResult {
                energy: -1.23,
                forces: vec![[0.0, 0.0, 0.0]],
                relaxed_candidate: candidate(),
                converged: true,
                wall_time: Duration::from_secs(1),
            }),
            artifacts: ExternalArtifacts {
                workdir: "/scratch/job-1/stage_01_attempt_01".into(),
                input_paths: vec!["/scratch/job-1/stage_01_attempt_01/candidate.gin".into()],
                output_paths: vec!["/scratch/job-1/stage_01_attempt_01/candidate.got".into()],
                stdout_path: Some("/scratch/job-1/stage_01_attempt_01/candidate.got".into()),
                stderr_path: None,
                artifact_records: Vec::new(),
            },
            submitted_run: None,
        }
    }

    #[test]
    fn parser_valid_completion_flows_into_scott_report() {
        let mut state = OrchestratedJobState::new("job-1", 10);
        state.status = OrchestratedJobStatus::RunningExternal;
        state.attempt = 1;

        let report = reconciled_stage_completion_to_runtime_report(
            &stage_job("job-1"),
            converged_outcome(),
            ReconciledStageCompletionContext {
                scheduler_receipt: Some(receipt(SchedulerReceiptState::Failed)),
                allocation_summary: Some(summary(AllocationTaskPhase::Completed)),
                orchestration_state: Some(state),
                prior_accepted: None,
            },
        )
        .expect("report");

        assert_eq!(report.status, ScottRuntimeStatus::Completed);
        assert!(report.stage_result.is_some());
        assert_eq!(
            report
                .diagnostics
                .metrics
                .get("reconciliation_authoritative_source")
                .map(String::as_str),
            Some("parser")
        );
    }

    #[test]
    fn allocation_failure_short_circuits_scott_completion() {
        let mut state = OrchestratedJobState::new("job-1", 10);
        state.status = OrchestratedJobStatus::RunningExternal;
        state.attempt = 1;

        let report = reconciled_stage_completion_to_runtime_report(
            &stage_job("job-1"),
            ExternalEvaluationOutcome {
                status: ExternalEvaluationStatus::Submitted,
                result: None,
                artifacts: ExternalArtifacts {
                    workdir: "/scratch/job-1/stage_01_attempt_01".into(),
                    input_paths: Vec::new(),
                    output_paths: Vec::new(),
                    stdout_path: None,
                    stderr_path: None,
                    artifact_records: Vec::new(),
                },
                submitted_run: None,
            },
            ReconciledStageCompletionContext {
                scheduler_receipt: Some(receipt(SchedulerReceiptState::Running)),
                allocation_summary: Some(summary(AllocationTaskPhase::Failed)),
                orchestration_state: Some(state),
                prior_accepted: None,
            },
        )
        .expect("report");

        assert_eq!(report.status, ScottRuntimeStatus::Failed);
        assert!(report.stage_result.is_none());
        assert_eq!(
            report
                .diagnostics
                .metrics
                .get("reconciliation_authoritative_source")
                .map(String::as_str),
            Some("allocation_runtime")
        );
    }
}
