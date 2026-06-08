use patina_evaluator::{
    normalize_backend_attempt, ProcedureAction, ProcedureCursor, ProcedureDecision,
    ProcedureRuntimeState, ScottEvalOutcome, ScottEvalRollback, ScottEvaluationState,
    ScottEvaluatorError, StageTicket, StageTransition,
};
use patina_external::{
    EvalError, ExternalEvaluationOutcome, ExternalEvaluationStatus, ExternalProgram,
};
use thiserror::Error;

use crate::backend_bridge::{ghost_result_to_attempt_report, GhostAttemptContext};
use crate::{
    RuntimeDiagnostics, ScottJobKind, ScottRuntimeJob, ScottRuntimeReport, ScottRuntimeStatus,
    ScottStageDispatch, ScottStageResult,
};

/// Persisted accepted Scott state carried into terminal stage completion.
#[derive(Debug, Clone, PartialEq)]
pub struct PriorAcceptedStageState {
    pub accepted_stage: patina_evaluator::StageIndex,
    pub accepted_result: patina_types::EvalResult,
}

/// Runtime-neutral context available when finalizing one terminal stage job.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StageJobCompletionContext {
    pub diagnostics: RuntimeDiagnostics,
    pub prior_accepted: Option<PriorAcceptedStageState>,
}

impl StageJobCompletionContext {
    pub fn with_diagnostics(diagnostics: RuntimeDiagnostics) -> Self {
        Self {
            diagnostics,
            prior_accepted: None,
        }
    }

    pub fn with_prior_accepted(mut self, prior_accepted: PriorAcceptedStageState) -> Self {
        self.prior_accepted = Some(prior_accepted);
        self
    }
}

/// Errors returned while turning a terminal external stage outcome into a Scott runtime report.
#[derive(Debug, Error)]
pub enum StageJobCompletionError {
    #[error("job `{job_id}` is not a Scott stage-worker job")]
    NotStageWorkerJob { job_id: String },
    #[error("job `{job_id}` references unknown Scott stage {stage}")]
    UnknownStage { job_id: String, stage: u8 },
    #[error(
        "job `{job_id}` cannot finalize deferred external outcome for `{program}` before terminal completion"
    )]
    DeferredExternalOutcome { job_id: String, program: String },
    #[error(
        "job `{job_id}` cannot materialize export-only external outcome for `{program}` as a Scott stage result"
    )]
    ExportOnlyExternalOutcome { job_id: String, program: String },
    #[error(
        "job `{job_id}` requires previously accepted Scott state to preserve a failed final refinement at stage {stage}"
    )]
    MissingPriorAcceptedContext { job_id: String, stage: u8 },
    #[error("scott procedure bridge failed: {0}")]
    Procedure(#[from] ScottEvaluatorError),
}

/// Converts one terminal external adapter outcome into a Scott runtime report for a stage job.
///
/// This bridge is intentionally narrow:
/// - it completes the currently dispatched stage attempt
/// - it can generate a retry or next-stage Scott job from the current dispatch
/// - it does not fabricate missing historical Scott state
///
/// That means decisions such as `keep_previous_accepted` still require a higher-level durable
/// checkpoint carrying the previously accepted Scott result.
pub fn external_stage_outcome_to_runtime_report(
    job: &ScottRuntimeJob,
    outcome: ExternalEvaluationOutcome,
    diagnostics: RuntimeDiagnostics,
) -> Result<ScottRuntimeReport, StageJobCompletionError> {
    external_stage_outcome_to_runtime_report_with_context(
        job,
        outcome,
        StageJobCompletionContext::with_diagnostics(diagnostics),
    )
}

/// Variant of terminal stage completion that also accepts persisted prior Scott acceptance.
pub fn external_stage_outcome_to_runtime_report_with_context(
    job: &ScottRuntimeJob,
    outcome: ExternalEvaluationOutcome,
    context: StageJobCompletionContext,
) -> Result<ScottRuntimeReport, StageJobCompletionError> {
    let dispatch = stage_dispatch(job)?;
    let ticket = dispatch.cursor.ticket.clone();
    let current_stage = dispatch.plan.stages.get(ticket.stage).ok_or_else(|| {
        StageJobCompletionError::UnknownStage {
            job_id: job.identity.job_id.clone(),
            stage: ticket.stage.get(),
        }
    })?;
    let program = ExternalProgram::from(current_stage.engine);
    let attempt_report = ghost_result_to_attempt_report(
        GhostAttemptContext {
            stage: ticket.stage,
            attempt: ticket.attempt,
            primary_output_path: primary_output_path(&outcome),
        },
        external_outcome_to_eval_result(&job.identity.job_id, program, outcome),
    );

    let is_final_stage = dispatch.plan.stages.next_after(ticket.stage).is_none();
    let (status, failure, decision) = normalize_backend_attempt(
        &attempt_report,
        dispatch.plan.evaluator.settings.max_relaxation_attempts,
        dispatch.plan.evaluator.settings.final_stage_failure_policy,
        is_final_stage,
    );

    let mut state = ScottEvaluationState::new(
        dispatch.request.request_id.clone(),
        dispatch.request.candidate.clone(),
        ticket.stage,
    );
    state.record_stage_evaluation(status, attempt_report.result.clone());
    if let Some(failure) = failure {
        state.record_failure(failure);
    }

    let followup_candidate = next_working_candidate(dispatch, &attempt_report);
    let (status, cursor, generated_jobs) = match decision {
        ProcedureDecision::AcceptStage => {
            if let Some(result) = attempt_report.result.clone() {
                state.set_accepted_result(ticket.stage, result);
            }
            let transition = accepted_transition(dispatch, &ticket, &attempt_report)?;
            match transition {
                StageTransition::Continue => {
                    let next_stage =
                        dispatch
                            .plan
                            .stages
                            .next_after(ticket.stage)
                            .ok_or_else(|| {
                                ScottEvaluatorError::InvalidStagePlan(
                                    patina_evaluator::StageSelectionError::UnknownStage(
                                        ticket.stage.get(),
                                    ),
                                )
                            })?;
                    state.current_stage = Some(next_stage.stage);
                    let next_ticket = StageTicket {
                        request_id: ticket.request_id.clone(),
                        backend_mode: ticket.backend_mode,
                        stage: next_stage.stage,
                        attempt: 1,
                        workdir: ticket.workdir.clone(),
                    };
                    let cursor = ProcedureCursor {
                        ticket: next_ticket.clone(),
                        state: ProcedureRuntimeState::ReadyToSubmit,
                    };
                    let next_job = followup_stage_job(
                        job,
                        dispatch,
                        followup_candidate,
                        next_ticket,
                        ProcedureAction::Submit(cursor.ticket.clone()),
                    );
                    (ScottRuntimeStatus::Completed, cursor, vec![next_job])
                }
                StageTransition::Stop => (
                    ScottRuntimeStatus::Completed,
                    ProcedureCursor {
                        ticket: ticket.clone(),
                        state: ProcedureRuntimeState::Completed,
                    },
                    Vec::new(),
                ),
            }
        }
        ProcedureDecision::RetryStage => {
            let retry_ticket = StageTicket {
                request_id: ticket.request_id.clone(),
                backend_mode: ticket.backend_mode,
                stage: ticket.stage,
                attempt: ticket.attempt + 1,
                workdir: ticket.workdir.clone(),
            };
            let cursor = ProcedureCursor {
                ticket: retry_ticket.clone(),
                state: ProcedureRuntimeState::ReadyToSubmit,
            };
            let retry_job = followup_stage_job(
                job,
                dispatch,
                followup_candidate,
                retry_ticket,
                ProcedureAction::Retry(cursor.ticket.clone()),
            );
            (ScottRuntimeStatus::Completed, cursor, vec![retry_job])
        }
        ProcedureDecision::RejectCandidate => {
            state.mark_relax_failed(ScottEvalRollback::for_failed_stage(
                ticket.stage,
                None,
                dispatch.plan.evaluator.settings.final_stage_failure_policy,
            ));
            (
                ScottRuntimeStatus::Failed,
                ProcedureCursor {
                    ticket: ticket.clone(),
                    state: ProcedureRuntimeState::Rejected,
                },
                Vec::new(),
            )
        }
        ProcedureDecision::KeepPreviousAccepted => {
            let Some(prior_accepted) = context.prior_accepted.as_ref() else {
                return Err(StageJobCompletionError::MissingPriorAcceptedContext {
                    job_id: job.identity.job_id.clone(),
                    stage: ticket.stage.get(),
                });
            };
            state.set_accepted_result(
                prior_accepted.accepted_stage,
                prior_accepted.accepted_result.clone(),
            );
            state.mark_relax_failed(ScottEvalRollback::for_failed_stage(
                ticket.stage,
                Some(prior_accepted.accepted_stage),
                dispatch.plan.evaluator.settings.final_stage_failure_policy,
            ));
            (
                ScottRuntimeStatus::Completed,
                ProcedureCursor {
                    ticket: ticket.clone(),
                    state: ProcedureRuntimeState::Completed,
                },
                Vec::new(),
            )
        }
    };

    let outcome = ScottEvalOutcome {
        final_result: state.accepted_result.clone(),
        final_stage: state.accepted_stage,
        relax_failed: state.relax_failed,
        provenance: state.provenance(),
        state,
    };

    Ok(ScottRuntimeReport {
        identity: job.identity.clone(),
        status,
        stage_result: Some(ScottStageResult {
            ticket,
            cursor,
            outcome,
        }),
        generated_jobs,
        diagnostics: context.diagnostics,
    })
}

fn stage_dispatch(job: &ScottRuntimeJob) -> Result<&ScottStageDispatch, StageJobCompletionError> {
    match &job.kind {
        ScottJobKind::EvaluateStage { dispatch } => Ok(dispatch),
        _ => Err(StageJobCompletionError::NotStageWorkerJob {
            job_id: job.identity.job_id.clone(),
        }),
    }
}

fn accepted_transition(
    dispatch: &ScottStageDispatch,
    ticket: &StageTicket,
    attempt_report: &patina_evaluator::BackendAttemptReport,
) -> Result<StageTransition, StageJobCompletionError> {
    let accepted_energy = attempt_report
        .result
        .as_ref()
        .map(|result| result.energy)
        .or(attempt_report.energy);
    accepted_energy
        .map(|energy| {
            dispatch
                .plan
                .stages
                .transition_for_energy(ticket.stage, energy)
                .map_err(ScottEvaluatorError::InvalidStagePlan)
                .map_err(Into::into)
        })
        .transpose()
        .map(|transition| transition.unwrap_or(StageTransition::Stop))
}

fn next_working_candidate(
    dispatch: &ScottStageDispatch,
    attempt_report: &patina_evaluator::BackendAttemptReport,
) -> patina_types::Candidate {
    if dispatch.plan.evaluator.settings.retrieve_relaxed_geometry {
        if let Some(result) = attempt_report.result.as_ref() {
            return result.relaxed_candidate.clone();
        }
    }
    dispatch.request.candidate.clone()
}

fn followup_stage_job(
    current_job: &ScottRuntimeJob,
    dispatch: &ScottStageDispatch,
    candidate: patina_types::Candidate,
    ticket: StageTicket,
    action: ProcedureAction,
) -> ScottRuntimeJob {
    let mut generated = ScottRuntimeJob::stage_worker(
        derived_stage_job_id(&ticket),
        current_job.identity.parent_job_id.clone(),
        ScottStageDispatch {
            plan: dispatch.plan.clone(),
            request: patina_evaluator::ScottProcedureRequest {
                candidate,
                request_id: dispatch.request.request_id.clone(),
                workdir: dispatch.request.workdir.clone(),
            },
            cursor: ProcedureCursor {
                ticket: ticket.clone(),
                state: ProcedureRuntimeState::ReadyToSubmit,
            },
            action,
        },
    );
    generated.requested_cores = current_job.requested_cores;
    generated.requested_gpus = current_job.requested_gpus;
    generated.required_tags = current_job.required_tags.clone();
    for (key, value) in &current_job.labels {
        generated.labels.entry(key.clone()).or_insert(value.clone());
    }
    generated
}

fn derived_stage_job_id(ticket: &StageTicket) -> String {
    format!(
        "{}:stage-{:02}:attempt-{:02}",
        ticket.request_id,
        ticket.stage.get(),
        ticket.attempt
    )
}

fn primary_output_path(outcome: &ExternalEvaluationOutcome) -> Option<String> {
    outcome
        .artifacts
        .output_paths
        .first()
        .or(outcome.artifacts.stdout_path.as_ref())
        .map(|path| path.display().to_string())
}

fn external_outcome_to_eval_result(
    job_id: &str,
    program: ExternalProgram,
    outcome: ExternalEvaluationOutcome,
) -> Result<patina_types::EvalResult, EvalError> {
    match outcome.status {
        ExternalEvaluationStatus::Converged => {
            outcome.result.ok_or_else(|| EvalError::ParseFailed {
                line: 0,
                reason: format!(
                    "job `{job_id}` external adapter `{}` converged without an EvalResult payload",
                    program.as_str()
                ),
            })
        }
        ExternalEvaluationStatus::NotConverged => Err(EvalError::NotConverged {
            energy: outcome
                .result
                .as_ref()
                .map(|result| result.energy)
                .unwrap_or(f64::NAN),
            n_steps: 0,
            partial_result: outcome.result.map(Box::new),
        }),
        ExternalEvaluationStatus::Submitted => Err(EvalError::ProcessFailed {
            exit_code: None,
            stderr: format!(
                "job `{job_id}` external adapter `{}` is still deferred",
                program.as_str()
            ),
        }),
        ExternalEvaluationStatus::ExportedOnly => Err(EvalError::ProcessFailed {
            exit_code: None,
            stderr: format!(
                "job `{job_id}` external adapter `{}` exported inputs without a terminal evaluation",
                program.as_str()
            ),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        external_stage_outcome_to_runtime_report,
        external_stage_outcome_to_runtime_report_with_context, PriorAcceptedStageState,
        StageJobCompletionContext, StageJobCompletionError,
    };
    use crate::{RuntimeDiagnostics, ScottRuntimeJob, ScottRuntimeStatus, ScottStageDispatch};
    use patina_evaluator::{
        FinalStageFailurePolicy, ProcedureAction, ProcedureCursor, ScottBackendMode,
        ScottEvaluatorPlan, ScottEvaluatorSettings, ScottLatticeMode, ScottProcedureIntent,
        ScottProcedurePlan, ScottProcedureRequest, StageEngine, StageIndex, StagePlan,
        StageSelection, StageTicket,
    };
    use patina_external::{
        ExternalArtifacts, ExternalEvaluationMode, ExternalEvaluationOutcome,
        ExternalEvaluationRequest, ExternalEvaluationStatus, ExternalEvaluator,
        ExternalExecutionMode, ExternalProgram, ExternalTemplateSet, GulpExternalAdapter,
        GulpExternalAdapterConfig,
    };
    use patina_types::{Candidate, EvalResult};
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::time::Duration;
    use tempfile::tempdir;

    fn candidate(label: &str) -> Candidate {
        Candidate::cluster(label, vec!["Mg".into()], vec![[0.0, 0.0, 0.0]])
    }

    fn periodic_candidate(label: &str) -> Candidate {
        Candidate::fully_periodic(
            label,
            vec!["Ce".into(), "O".into()],
            vec![[0.0, 0.0, 0.0], [0.5, 0.5, 0.5]],
            [[5.4, 0.0, 0.0], [0.0, 5.4, 0.0], [0.0, 0.0, 5.4]],
        )
    }

    fn plan(final_policy: FinalStageFailurePolicy, stages: Vec<StagePlan>) -> ScottProcedurePlan {
        ScottProcedurePlan {
            evaluator: ScottEvaluatorPlan {
                master_template: "/tmp/Master.gin".into(),
                atoms_in: None,
                work_root: "/tmp".into(),
                settings: ScottEvaluatorSettings {
                    backend_mode: ScottBackendMode::Gulp,
                    procedure_intent: ScottProcedureIntent::ProductionRun,
                    lattice_mode: ScottLatticeMode::Cluster,
                    max_relaxation_attempts: 2,
                    gnorm_tolerance: 1.0e-4,
                    final_stage_failure_policy: final_policy,
                    retrieve_relaxed_geometry: true,
                    keep_stage_artifacts: true,
                },
            },
            stages: StageSelection { stages },
        }
    }

    fn dispatch(
        plan: ScottProcedurePlan,
        request_id: &str,
        candidate: Candidate,
        stage: u8,
        attempt: usize,
    ) -> ScottStageDispatch {
        let ticket = StageTicket {
            request_id: request_id.into(),
            backend_mode: ScottBackendMode::Gulp,
            stage: StageIndex(stage),
            attempt,
            workdir: format!("/tmp/{request_id}").into(),
        };
        ScottStageDispatch {
            plan,
            request: ScottProcedureRequest {
                candidate,
                request_id: request_id.into(),
                workdir: format!("/tmp/{request_id}").into(),
            },
            cursor: ProcedureCursor::new(ticket.clone()),
            action: ProcedureAction::Submit(ticket),
        }
    }

    fn stage_job(
        request_id: &str,
        candidate: Candidate,
        stage: u8,
        attempt: usize,
        stages: Vec<StagePlan>,
        final_policy: FinalStageFailurePolicy,
    ) -> ScottRuntimeJob {
        let dispatch = dispatch(
            plan(final_policy, stages),
            request_id,
            candidate,
            stage,
            attempt,
        );
        let mut job = ScottRuntimeJob::stage_worker(
            format!("{request_id}:stage-{stage}-attempt-{attempt}"),
            Some(format!("controller:{request_id}")),
            dispatch,
        );
        job.requested_cores = 8;
        job.required_tags = vec!["cpu".into(), "gulp".into()];
        job
    }

    fn converged_outcome(result: EvalResult) -> ExternalEvaluationOutcome {
        ExternalEvaluationOutcome {
            status: ExternalEvaluationStatus::Converged,
            result: Some(result),
            artifacts: ExternalArtifacts {
                workdir: "/tmp/run".into(),
                input_paths: vec!["/tmp/run/candidate.gin".into()],
                output_paths: vec!["/tmp/run/candidate.got".into()],
                stdout_path: Some("/tmp/run/candidate.got".into()),
                stderr_path: Some("/tmp/run/gulp.stderr.log".into()),
                artifact_records: Vec::new(),
            },
            submitted_run: None,
        }
    }

    fn not_converged_outcome(result: EvalResult) -> ExternalEvaluationOutcome {
        ExternalEvaluationOutcome {
            status: ExternalEvaluationStatus::NotConverged,
            result: Some(result),
            artifacts: ExternalArtifacts {
                workdir: "/tmp/run".into(),
                input_paths: vec!["/tmp/run/candidate.gin".into()],
                output_paths: vec!["/tmp/run/candidate.got".into()],
                stdout_path: Some("/tmp/run/candidate.got".into()),
                stderr_path: Some("/tmp/run/gulp.stderr.log".into()),
                artifact_records: Vec::new(),
            },
            submitted_run: None,
        }
    }

    fn result(label: &str, energy: f64, converged: bool) -> EvalResult {
        EvalResult {
            energy,
            forces: vec![[0.0, 0.0, 0.0]],
            relaxed_candidate: candidate(label),
            converged,
            wall_time: Duration::from_secs(2),
        }
    }

    #[test]
    fn accepted_stage_generates_next_stage_job() {
        let job = stage_job(
            "req-next",
            candidate("start"),
            1,
            1,
            vec![
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
            FinalStageFailurePolicy::RejectCandidate,
        );

        let report = external_stage_outcome_to_runtime_report(
            &job,
            converged_outcome(result("after-stage-1", -12.0, true)),
            RuntimeDiagnostics::default(),
        )
        .expect("report");

        assert_eq!(report.status, ScottRuntimeStatus::Completed);
        assert_eq!(report.generated_jobs.len(), 1);
        let next_job = &report.generated_jobs[0];
        assert_eq!(
            next_job.identity.candidate_label.as_deref(),
            Some("after-stage-1")
        );
        match &next_job.kind {
            crate::ScottJobKind::EvaluateStage { dispatch } => {
                assert_eq!(dispatch.cursor.ticket.stage, StageIndex(2));
                assert_eq!(dispatch.cursor.ticket.attempt, 1);
            }
            other => panic!("expected stage job, got {other:?}"),
        }
    }

    #[test]
    fn retryable_stage_generates_retry_job() {
        let job = stage_job(
            "req-retry",
            candidate("start"),
            1,
            1,
            vec![StagePlan {
                stage: StageIndex(1),
                engine: StageEngine::Gulp,
                refine_if_energy_below: None,
                energy_min_threshold: None,
                energy_max_threshold: None,
                keep_only_if_final_stage: false,
            }],
            FinalStageFailurePolicy::RejectCandidate,
        );

        let report = external_stage_outcome_to_runtime_report(
            &job,
            not_converged_outcome(result("partially-relaxed", -4.0, false)),
            RuntimeDiagnostics::default(),
        )
        .expect("report");

        assert_eq!(report.status, ScottRuntimeStatus::Completed);
        assert_eq!(report.generated_jobs.len(), 1);
        let retry_job = &report.generated_jobs[0];
        assert_eq!(
            retry_job.identity.candidate_label.as_deref(),
            Some("partially-relaxed")
        );
        match &retry_job.kind {
            crate::ScottJobKind::EvaluateStage { dispatch } => {
                assert_eq!(dispatch.cursor.ticket.stage, StageIndex(1));
                assert_eq!(dispatch.cursor.ticket.attempt, 2);
                assert!(matches!(dispatch.action, ProcedureAction::Retry(_)));
            }
            other => panic!("expected retry stage job, got {other:?}"),
        }
    }

    #[test]
    fn keep_previous_accepted_requires_prior_checkpointed_state() {
        let job = stage_job(
            "req-keep",
            candidate("stage2-input"),
            2,
            2,
            vec![
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
            FinalStageFailurePolicy::KeepPreviousAccepted,
        );

        let error = external_stage_outcome_to_runtime_report(
            &job,
            ExternalEvaluationOutcome {
                status: ExternalEvaluationStatus::NotConverged,
                result: None,
                artifacts: ExternalArtifacts::default(),
                submitted_run: None,
            },
            RuntimeDiagnostics::default(),
        )
        .expect_err("missing prior accepted state should fail");

        assert!(matches!(
            error,
            StageJobCompletionError::MissingPriorAcceptedContext { stage: 2, .. }
        ));
    }

    #[test]
    fn keep_previous_accepted_uses_persisted_prior_state_when_available() {
        let job = stage_job(
            "req-keep-ok",
            candidate("stage2-input"),
            2,
            2,
            vec![
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
            FinalStageFailurePolicy::KeepPreviousAccepted,
        );

        let report = external_stage_outcome_to_runtime_report_with_context(
            &job,
            ExternalEvaluationOutcome {
                status: ExternalEvaluationStatus::NotConverged,
                result: None,
                artifacts: ExternalArtifacts::default(),
                submitted_run: None,
            },
            StageJobCompletionContext::with_diagnostics(RuntimeDiagnostics::default())
                .with_prior_accepted(PriorAcceptedStageState {
                    accepted_stage: StageIndex(1),
                    accepted_result: result("accepted-stage-1", -9.5, true),
                }),
        )
        .expect("prior accepted context should preserve acceptance");

        assert_eq!(report.status, ScottRuntimeStatus::Completed);
        assert!(report.generated_jobs.is_empty());
        let stage_result = report.stage_result.expect("stage result");
        assert_eq!(stage_result.outcome.final_stage, Some(StageIndex(1)));
        assert!(stage_result.outcome.relax_failed);
        assert_eq!(
            stage_result
                .outcome
                .final_result
                .as_ref()
                .map(|result| result.energy),
            Some(-9.5)
        );
    }

    #[test]
    #[cfg(unix)]
    fn concrete_gulp_adapter_outcome_maps_to_runtime_report() {
        let temp = tempdir().expect("tempdir");
        let template_path = temp.path().join("template.gin");
        fs::write(
            &template_path,
            "\
opti conp
# KLMC3-RS EXPECT_ATOMS: 2
# === KLMC3-RS COORDINATE INJECTION POINT ===
",
        )
        .expect("write template");
        let script_path = temp.path().join("fake_gulp.sh");
        fs::write(
            &script_path,
            "#!/bin/sh\nprintf 'Final energy = -1.23 eV\nOptimisation achieved\nFinal fractional coordinates of atoms\n 1 Ce 0.0 0.0 0.0\n 2 O 0.5 0.5 0.5\n' > candidate.got\n",
        )
        .expect("write script");
        let mut perms = fs::metadata(&script_path).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).expect("chmod");

        let adapter = GulpExternalAdapter::new(
            GulpExternalAdapterConfig::new(&template_path, &script_path)
                .with_timeout(Some(Duration::from_secs(5))),
        )
        .expect("adapter");

        let run_dir = temp.path().join("run-local");
        let external_request = ExternalEvaluationRequest {
            request_id: "req-gulp-runtime".into(),
            stage: 1,
            program: ExternalProgram::Gulp,
            mode: ExternalEvaluationMode::Relaxation,
            execution: ExternalExecutionMode::LocalScript,
            retrieve_relaxed_geometry: true,
            candidate: periodic_candidate("ceria"),
            workdir: run_dir.clone(),
            templates: ExternalTemplateSet::default(),
        };
        let external_outcome = adapter.evaluate(&external_request).expect("gulp outcome");

        let job = stage_job(
            "req-gulp-runtime",
            periodic_candidate("ceria"),
            1,
            1,
            vec![StagePlan {
                stage: StageIndex(1),
                engine: StageEngine::Gulp,
                refine_if_energy_below: None,
                energy_min_threshold: None,
                energy_max_threshold: None,
                keep_only_if_final_stage: false,
            }],
            FinalStageFailurePolicy::RejectCandidate,
        );

        let report = external_stage_outcome_to_runtime_report(
            &job,
            external_outcome,
            RuntimeDiagnostics::default(),
        )
        .expect("runtime report");

        assert_eq!(report.status, ScottRuntimeStatus::Completed);
        assert!(report.generated_jobs.is_empty());
        let stage_result = report.stage_result.expect("stage result");
        assert_eq!(stage_result.ticket.stage, StageIndex(1));
        assert_eq!(stage_result.outcome.final_stage, Some(StageIndex(1)));
        assert_eq!(
            stage_result
                .outcome
                .final_result
                .as_ref()
                .map(|result| result.energy),
            Some(-1.23)
        );
        assert!(run_dir.join("candidate.gin").exists());
    }

    #[allow(dead_code)]
    fn _path(_: &Path) {}
}
