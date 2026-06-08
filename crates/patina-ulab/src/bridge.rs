use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;

use patina_runtime::{ScottJobKind, ScottRuntimeJob};

/// Static projection knobs for mapping Scott jobs into a `unified_lab`-style substrate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnifiedLabBridgeConfig {
    pub stage_engine_name: String,
    pub controller_engine_name: String,
    pub stage_required_tags: Vec<String>,
    pub controller_required_tags: Vec<String>,
}

impl Default for UnifiedLabBridgeConfig {
    fn default() -> Self {
        Self {
            stage_engine_name: "scott-stage-worker".into(),
            controller_engine_name: "scott-controller".into(),
            stage_required_tags: vec!["muscle".into()],
            controller_required_tags: vec!["brain".into()],
        }
    }
}

/// Projection of a Scott worker-stage job into a neutral `unified_lab`-style payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnifiedLabWorkerEnvelope {
    pub job_id: String,
    pub parent_job_id: Option<String>,
    pub engine_name: String,
    pub required_tags: Vec<String>,
    pub requested_cores: usize,
    pub requested_gpus: usize,
    pub labels: IndexMap<String, String>,
    pub params: Value,
}

/// Projection of a Scott controller job into a neutral workflow payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnifiedLabWorkflowEnvelope {
    pub job_id: String,
    pub engine_name: String,
    pub required_tags: Vec<String>,
    pub requested_cores: usize,
    pub requested_gpus: usize,
    pub labels: IndexMap<String, String>,
    pub params: Value,
}

#[derive(Debug, Error)]
pub enum UnifiedLabProjectionError {
    #[error("job `{job_id}` cannot be projected as a worker envelope because it is not a stage-worker job")]
    NotAWorkerStageJob { job_id: String },
    #[error("job `{job_id}` cannot be projected as a controller envelope because it is not a controller job")]
    NotAControllerJob { job_id: String },
    #[error("failed to serialize projected Scott payload: {0}")]
    Serialize(#[from] serde_json::Error),
}

/// Optional bridge that projects Scott runtime jobs into `unified_lab`-friendly payloads.
///
/// This intentionally stays independent from concrete `unified_lab` Rust types so the two
/// systems can evolve without creating a hard compile-time knot too early.
#[derive(Debug, Clone)]
pub struct UnifiedLabRuntimeBridge {
    cfg: UnifiedLabBridgeConfig,
}

impl UnifiedLabRuntimeBridge {
    pub fn new(cfg: UnifiedLabBridgeConfig) -> Self {
        Self { cfg }
    }

    pub fn project_worker(
        &self,
        job: &ScottRuntimeJob,
    ) -> Result<UnifiedLabWorkerEnvelope, UnifiedLabProjectionError> {
        match &job.kind {
            ScottJobKind::EvaluateStage { dispatch } => Ok(UnifiedLabWorkerEnvelope {
                job_id: job.identity.job_id.clone(),
                parent_job_id: job.identity.parent_job_id.clone(),
                engine_name: self.cfg.stage_engine_name.clone(),
                required_tags: if job.required_tags.is_empty() {
                    self.cfg.stage_required_tags.clone()
                } else {
                    job.required_tags.clone()
                },
                requested_cores: job.requested_cores,
                requested_gpus: job.requested_gpus,
                labels: job.labels.clone(),
                params: json!({
                    "role": "scott_worker",
                    "candidate_label": job.identity.candidate_label,
                    "dispatch": dispatch,
                }),
            }),
            _ => Err(UnifiedLabProjectionError::NotAWorkerStageJob {
                job_id: job.identity.job_id.clone(),
            }),
        }
    }

    pub fn project_controller(
        &self,
        job: &ScottRuntimeJob,
    ) -> Result<UnifiedLabWorkflowEnvelope, UnifiedLabProjectionError> {
        match &job.kind {
            ScottJobKind::Controller { kind, resume_from } => Ok(UnifiedLabWorkflowEnvelope {
                job_id: job.identity.job_id.clone(),
                engine_name: self.cfg.controller_engine_name.clone(),
                required_tags: if job.required_tags.is_empty() {
                    self.cfg.controller_required_tags.clone()
                } else {
                    job.required_tags.clone()
                },
                requested_cores: job.requested_cores,
                requested_gpus: job.requested_gpus,
                labels: job.labels.clone(),
                params: json!({
                    "role": "scott_controller",
                    "controller_kind": kind,
                    "resume_from": resume_from,
                }),
            }),
            _ => Err(UnifiedLabProjectionError::NotAControllerJob {
                job_id: job.identity.job_id.clone(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{UnifiedLabBridgeConfig, UnifiedLabProjectionError, UnifiedLabRuntimeBridge};
    use indexmap::IndexMap;
    use patina_evaluator::{
        FinalStageFailurePolicy, ProcedureAction, ProcedureCursor, ScottBackendMode,
        ScottEvaluatorPlan, ScottEvaluatorSettings, ScottLatticeMode, ScottProcedureIntent,
        ScottProcedurePlan, ScottProcedureRequest, StageEngine, StageIndex, StagePlan,
        StageSelection, StageTicket,
    };
    use patina_runtime::{
        RuntimeCheckpoint, ScottControllerKind, ScottJobIdentity, ScottJobKind, ScottJobRole,
        ScottRuntimeJob, ScottRuntimeStatus, ScottStageDispatch,
    };
    use patina_types::Candidate;

    fn candidate() -> Candidate {
        Candidate::cluster("o1", vec!["O".into()], vec![[0.0, 0.0, 0.0]])
    }

    fn stage_job() -> ScottRuntimeJob {
        let ticket = StageTicket {
            request_id: "req-1".into(),
            backend_mode: ScottBackendMode::JanusMace,
            stage: StageIndex(1),
            attempt: 1,
            workdir: "/tmp/stage".into(),
        };
        ScottRuntimeJob::stage_worker(
            "job-1",
            Some("parent-1".into()),
            ScottStageDispatch {
                plan: ScottProcedurePlan {
                    evaluator: ScottEvaluatorPlan {
                        master_template: "/tmp/Master.gin".into(),
                        atoms_in: None,
                        work_root: "/tmp".into(),
                        settings: ScottEvaluatorSettings {
                            backend_mode: ScottBackendMode::JanusMace,
                            procedure_intent: ScottProcedureIntent::ProductionRun,
                            lattice_mode: ScottLatticeMode::Cluster,
                            max_relaxation_attempts: 1,
                            gnorm_tolerance: 1.0e-4,
                            final_stage_failure_policy:
                                FinalStageFailurePolicy::KeepPreviousAccepted,
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
                    workdir: "/tmp/stage".into(),
                },
                cursor: ProcedureCursor::new(ticket.clone()),
                action: ProcedureAction::Submit(ticket),
            },
        )
    }

    fn controller_job() -> ScottRuntimeJob {
        ScottRuntimeJob {
            identity: ScottJobIdentity {
                job_id: "controller-1".into(),
                parent_job_id: None,
                candidate_label: None,
            },
            role: ScottJobRole::Controller,
            kind: ScottJobKind::Controller {
                kind: ScottControllerKind::BasinHopping,
                resume_from: Some(Box::new(RuntimeCheckpoint {
                    job: stage_job(),
                    dispatch_receipt: None,
                    status: ScottRuntimeStatus::Pending,
                })),
            },
            requested_cores: 1,
            requested_gpus: 0,
            required_tags: Vec::new(),
            labels: IndexMap::new(),
        }
    }

    #[test]
    fn bridge_projects_stage_jobs_to_worker_envelopes() {
        let bridge = UnifiedLabRuntimeBridge::new(UnifiedLabBridgeConfig::default());
        let projected = bridge.project_worker(&stage_job()).unwrap();
        assert_eq!(projected.engine_name, "scott-stage-worker");
        assert_eq!(projected.required_tags, vec!["muscle"]);
    }

    #[test]
    fn bridge_projects_controller_jobs_to_workflow_envelopes() {
        let bridge = UnifiedLabRuntimeBridge::new(UnifiedLabBridgeConfig::default());
        let projected = bridge.project_controller(&controller_job()).unwrap();
        assert_eq!(projected.engine_name, "scott-controller");
        assert_eq!(projected.required_tags, vec!["brain"]);
    }

    #[test]
    fn bridge_rejects_wrong_projection_direction() {
        let bridge = UnifiedLabRuntimeBridge::new(UnifiedLabBridgeConfig::default());
        let error = bridge.project_controller(&stage_job()).unwrap_err();
        assert!(matches!(
            error,
            UnifiedLabProjectionError::NotAControllerJob { .. }
        ));
    }
}
