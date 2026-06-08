use camino::Utf8PathBuf;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use patina_evaluator::{
    ProcedureAction, ProcedureCursor, ScottEvalOutcome, ScottProcedurePlan, ScottProcedureRequest,
    StageTicket,
};
use patina_external::ExternalProgram;

/// Stable runtime identity for a Scott controller or worker job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScottJobIdentity {
    pub job_id: String,
    pub parent_job_id: Option<String>,
    pub candidate_label: Option<String>,
}

/// Scott-owned role within a larger orchestration topology.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScottJobRole {
    Controller,
    Worker,
}

/// Higher-level Scott controller domains that may spawn further Scott jobs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScottControllerKind {
    ProductionRun,
    BasinHopping,
    GeneticAlgorithm,
    SolidSolutions,
    ScanSurface,
    SimulatedAnnealing,
    EnergyLid,
    HybridGaProduction,
}

/// Scott job kinds emitted by the domain layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ScottJobKind {
    Controller {
        kind: ScottControllerKind,
        resume_from: Option<Box<RuntimeCheckpoint>>,
    },
    EvaluateStage {
        dispatch: Box<ScottStageDispatch>,
    },
}

/// Fully specified stage-evaluation job emitted by Scott.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScottStageDispatch {
    pub plan: ScottProcedurePlan,
    pub request: ScottProcedureRequest,
    pub cursor: ProcedureCursor,
    pub action: ProcedureAction,
}

/// Runtime-submitted Scott job envelope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScottRuntimeJob {
    pub identity: ScottJobIdentity,
    pub role: ScottJobRole,
    pub kind: ScottJobKind,
    pub requested_cores: usize,
    pub requested_gpus: usize,
    pub required_tags: Vec<String>,
    pub labels: IndexMap<String, String>,
}

impl ScottRuntimeJob {
    pub fn stage_worker(
        job_id: impl Into<String>,
        parent_job_id: Option<String>,
        dispatch: ScottStageDispatch,
    ) -> Self {
        let ticket = &dispatch.cursor.ticket;
        let mut labels = IndexMap::new();
        labels.insert("request_id".into(), ticket.request_id.clone());
        labels.insert("stage".into(), ticket.stage.get().to_string());
        labels.insert("backend_mode".into(), format!("{:?}", ticket.backend_mode));
        if let Some(stage_plan) = dispatch.plan.stages.get(ticket.stage) {
            labels.insert(
                "external_program".into(),
                ExternalProgram::from(stage_plan.engine).as_str().into(),
            );
        }

        Self {
            identity: ScottJobIdentity {
                job_id: job_id.into(),
                parent_job_id,
                candidate_label: Some(dispatch.request.candidate.label.clone()),
            },
            role: ScottJobRole::Worker,
            kind: ScottJobKind::EvaluateStage {
                dispatch: Box::new(dispatch),
            },
            requested_cores: 1,
            requested_gpus: 0,
            required_tags: Vec::new(),
            labels,
        }
    }
}

/// Runtime-visible status independent of any specific scheduler implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScottRuntimeStatus {
    Pending,
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// Scheduler-neutral launch provenance attached to runtime diagnostics.
///
/// This deliberately uses strings rather than site-specific enums so Scott runtime reports can
/// preserve execution reality without depending directly on `patina-ulab`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RuntimeLaunchProvenance {
    pub site_name: Option<String>,
    pub scheduler_family: Option<String>,
    pub launch_strategy: Option<String>,
    pub launch_command: Option<String>,
    pub submit_command: Option<String>,
    pub status_command: Option<String>,
    pub accounting_command: Option<String>,
    pub cancel_command: Option<String>,
    pub telemetry_command: Option<String>,
    pub scheduler_job_id_env: Option<String>,
    pub module_environment: Option<String>,
    pub scratch_mode: Option<String>,
    pub preferred_submission_mode: Option<String>,
    pub poll_interval_secs: Option<u64>,
}

/// Scheduler/runtime diagnostics carried alongside Scott job reports.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RuntimeDiagnostics {
    pub worker_id: Option<String>,
    pub workdir: Option<Utf8PathBuf>,
    pub message: Option<String>,
    pub launch_provenance: Option<RuntimeLaunchProvenance>,
    pub metrics: IndexMap<String, String>,
}

impl RuntimeDiagnostics {
    pub fn with_launch_provenance(mut self, provenance: RuntimeLaunchProvenance) -> Self {
        self.launch_provenance = Some(provenance);
        self
    }
}

/// Stage result payload returned from a runtime worker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScottStageResult {
    pub ticket: StageTicket,
    pub cursor: ProcedureCursor,
    pub outcome: ScottEvalOutcome,
}

/// Unified report returned by a runtime substrate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScottRuntimeReport {
    pub identity: ScottJobIdentity,
    pub status: ScottRuntimeStatus,
    pub stage_result: Option<ScottStageResult>,
    pub generated_jobs: Vec<ScottRuntimeJob>,
    pub diagnostics: RuntimeDiagnostics,
}

/// Stable handle returned when a runtime accepts a Scott job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScottDispatchReceipt {
    pub job_id: String,
    pub runtime_handle: String,
}

/// Serializable Scott-side checkpoint fragment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeCheckpoint {
    pub job: ScottRuntimeJob,
    pub dispatch_receipt: Option<ScottDispatchReceipt>,
    pub status: ScottRuntimeStatus,
}

/// Errors returned by the Scott runtime abstraction.
#[derive(Debug, Error)]
pub enum ScottRuntimeError {
    #[error("runtime rejected job `{job_id}`: {message}")]
    SubmitRejected { job_id: String, message: String },
    #[error("runtime report for `{job_id}` was not ready yet")]
    ReportPending { job_id: String },
    #[error("runtime handle `{handle}` was unknown")]
    UnknownHandle { handle: String },
    #[error("runtime operation not implemented yet: {0}")]
    NotImplemented(&'static str),
}

/// Errors returned while rebuilding runtime checkpoints.
#[derive(Debug, Error)]
pub enum RuntimeCheckpointError {
    #[error("checkpoint for job `{job_id}` does not contain a runtime receipt")]
    MissingReceipt { job_id: String },
}

/// Scott-owned runtime contract.
///
/// Implementations can be local/blocking, Rayon-backed, durable, or bridged onto
/// an external orchestrator. Scott scientific code should depend on this trait
/// instead of directly depending on scheduler internals.
pub trait ScottRuntime: Send + Sync {
    fn submit(&self, job: &ScottRuntimeJob) -> Result<ScottDispatchReceipt, ScottRuntimeError>;

    fn poll(
        &self,
        receipt: &ScottDispatchReceipt,
    ) -> Result<Option<ScottRuntimeReport>, ScottRuntimeError>;

    fn cancel(&self, receipt: &ScottDispatchReceipt) -> Result<(), ScottRuntimeError>;
}

#[cfg(test)]
mod tests {
    use super::{
        RuntimeDiagnostics, RuntimeLaunchProvenance, ScottControllerKind, ScottJobIdentity,
        ScottJobKind, ScottJobRole, ScottRuntimeJob, ScottRuntimeStatus, ScottStageDispatch,
    };
    use indexmap::IndexMap;
    use patina_evaluator::{
        FinalStageFailurePolicy, ProcedureAction, ProcedureCursor, ScottBackendMode,
        ScottEvaluatorPlan, ScottEvaluatorSettings, ScottLatticeMode, ScottProcedureIntent,
        ScottProcedurePlan, ScottProcedureRequest, StageEngine, StageIndex, StagePlan,
        StageSelection, StageTicket,
    };
    use patina_types::Candidate;

    fn candidate() -> Candidate {
        Candidate::cluster("mg1", vec!["Mg".into()], vec![[0.0, 0.0, 0.0]])
    }

    fn stage_dispatch() -> ScottStageDispatch {
        let ticket = StageTicket {
            request_id: "req-1".into(),
            backend_mode: ScottBackendMode::Gulp,
            stage: StageIndex(1),
            attempt: 1,
            workdir: "/tmp/scott-stage".into(),
        };
        ScottStageDispatch {
            plan: ScottProcedurePlan {
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
                    stages: vec![StagePlan {
                        stage: StageIndex(1),
                        engine: StageEngine::Gulp,
                        refine_if_energy_below: None,
                        energy_min_threshold: None,
                        energy_max_threshold: None,
                        keep_only_if_final_stage: false,
                    }],
                },
            },
            request: ScottProcedureRequest {
                candidate: candidate(),
                request_id: "req-1".into(),
                workdir: "/tmp/scott-stage".into(),
            },
            cursor: ProcedureCursor::new(ticket.clone()),
            action: ProcedureAction::Submit(ticket),
        }
    }

    #[test]
    fn runtime_diagnostics_can_carry_launch_provenance() {
        let diagnostics =
            RuntimeDiagnostics::default().with_launch_provenance(RuntimeLaunchProvenance {
                site_name: Some("young".into()),
                scheduler_family: Some("slurm".into()),
                launch_strategy: Some("mpirun".into()),
                launch_command: Some("mpirun".into()),
                submit_command: Some("sbatch".into()),
                status_command: Some("squeue".into()),
                accounting_command: Some("sacct".into()),
                cancel_command: Some("scancel".into()),
                telemetry_command: Some("sstat".into()),
                scheduler_job_id_env: Some("SLURM_JOB_ID".into()),
                module_environment: Some("environment_modules".into()),
                scratch_mode: Some("shared_filesystem".into()),
                preferred_submission_mode: Some("within_allocation".into()),
                poll_interval_secs: Some(20),
            });

        assert_eq!(
            diagnostics
                .launch_provenance
                .as_ref()
                .and_then(|p| p.site_name.as_deref()),
            Some("young")
        );
        assert_eq!(
            diagnostics
                .launch_provenance
                .as_ref()
                .and_then(|p| p.telemetry_command.as_deref()),
            Some("sstat")
        );
    }

    #[test]
    fn stage_worker_job_captures_stage_metadata_in_labels() {
        let job = ScottRuntimeJob::stage_worker("job-1", Some("parent-1".into()), stage_dispatch());
        assert_eq!(job.role, ScottJobRole::Worker);
        assert_eq!(job.identity.job_id, "job-1");
        assert_eq!(job.identity.parent_job_id.as_deref(), Some("parent-1"));
        assert_eq!(
            job.labels.get("request_id").map(String::as_str),
            Some("req-1")
        );
        assert_eq!(job.labels.get("stage").map(String::as_str), Some("1"));
        assert_eq!(
            job.labels.get("external_program").map(String::as_str),
            Some("gulp")
        );
    }

    #[test]
    fn controller_jobs_remain_distinct_from_stage_jobs() {
        let job = ScottRuntimeJob {
            identity: ScottJobIdentity {
                job_id: "controller-1".into(),
                parent_job_id: None,
                candidate_label: None,
            },
            role: ScottJobRole::Controller,
            kind: ScottJobKind::Controller {
                kind: ScottControllerKind::GeneticAlgorithm,
                resume_from: None,
            },
            requested_cores: 1,
            requested_gpus: 0,
            required_tags: vec!["brain".into()],
            labels: IndexMap::new(),
        };

        assert_eq!(job.role, ScottJobRole::Controller);
        assert!(matches!(
            job.kind,
            ScottJobKind::Controller {
                kind: ScottControllerKind::GeneticAlgorithm,
                ..
            }
        ));
        assert_eq!(ScottRuntimeStatus::Pending, ScottRuntimeStatus::Pending);
    }
}
