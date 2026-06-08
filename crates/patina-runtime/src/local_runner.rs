use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use indexmap::IndexMap;
use patina_evaluator::{
    ProcedureAction, ProcedureState, ScottBackendMode, ScottEvalOutcome, ScottEvaluatorError,
    ScottProcedurePlan, ScottProcedureRequest, StageTicket,
};
use patina_external::{
    BackendEvaluator, BackendEvaluatorExternalAdapter, EvalError, ExternalError,
    ExternalEvaluationMode, ExternalEvaluationRequest, ExternalEvaluationStatus, ExternalEvaluator,
    ExternalExecutionMode, ExternalProgram, ExternalTemplateSet, JanusMaceBackend, JanusMaceConfig,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::backend_bridge::{ghost_result_to_attempt_report, GhostAttemptContext};

/// Runtime-side abstraction for evaluating one Scott stage attempt.
pub trait BackendStageExecutor: Send + Sync {
    fn evaluate_stage(
        &self,
        plan: &ScottProcedurePlan,
        request: &ScottProcedureRequest,
        ticket: &StageTicket,
    ) -> Result<patina_types::EvalResult, EvalError>;

    fn external_program_for_stage(
        &self,
        plan: &ScottProcedurePlan,
        ticket: &StageTicket,
    ) -> Result<ExternalProgram, EvalError> {
        stage_engine_external_program(plan, ticket)
    }

    fn primary_output_path(
        &self,
        plan: &ScottProcedurePlan,
        ticket: &StageTicket,
    ) -> Result<PathBuf, EvalError> {
        let program = self.external_program_for_stage(plan, ticket)?;
        Ok(default_stage_primary_output_path_for_program(
            program, ticket,
        ))
    }
}

/// Simple executor that reuses one `BackendEvaluator` for all Scott stages.
///
/// This is sufficient for the first end-to-end local Scott loop. More specific
/// stage-aware routing can layer on top later.
#[derive(Clone)]
pub struct FixedBackendStageExecutor {
    backend: Arc<dyn BackendEvaluator>,
}

impl FixedBackendStageExecutor {
    pub fn new(backend: Arc<dyn BackendEvaluator>) -> Self {
        Self { backend }
    }
}

impl std::fmt::Debug for FixedBackendStageExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FixedBackendStageExecutor").finish()
    }
}

impl BackendStageExecutor for FixedBackendStageExecutor {
    fn evaluate_stage(
        &self,
        _plan: &ScottProcedurePlan,
        request: &ScottProcedureRequest,
        ticket: &StageTicket,
    ) -> Result<patina_types::EvalResult, EvalError> {
        let stage_workdir = stage_attempt_dir(ticket);
        fs::create_dir_all(&stage_workdir).map_err(EvalError::IoError)?;
        self.backend.evaluate(&request.candidate, &stage_workdir)
    }
}

/// Configuration-driven routing policy from Scott stages to concrete backend modes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScottBackendRoutingPolicy {
    pub default_backend: ScottBackendMode,
    pub stage_overrides: IndexMap<u8, ScottBackendMode>,
}

impl ScottBackendRoutingPolicy {
    pub fn from_plan(plan: &ScottProcedurePlan) -> Self {
        Self {
            default_backend: plan.evaluator.settings.backend_mode,
            stage_overrides: IndexMap::new(),
        }
    }

    pub fn with_stage_backend(mut self, stage: u8, backend: ScottBackendMode) -> Self {
        self.stage_overrides.insert(stage, backend);
        self
    }

    pub fn backend_override_for_stage(&self, ticket: &StageTicket) -> Option<ScottBackendMode> {
        self.stage_overrides.get(&ticket.stage.get()).copied()
    }

    pub fn backend_for_stage(&self, ticket: &StageTicket) -> ScottBackendMode {
        self.backend_override_for_stage(ticket)
            .unwrap_or(self.default_backend)
    }
}

/// Backend-aware retry policy for Janus/MACE in Scott-staged execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct JanusRetryPolicy {
    /// Additional multiples of the base Janus step budget to add per retry.
    ///
    /// With base steps `80` and value `1`:
    /// - attempt 1 => `80`
    /// - attempt 2 => `160`
    /// - attempt 3 => `240`
    pub additional_step_multiples_per_retry: usize,
    /// Optional hard cap on the Janus step budget.
    pub max_steps: Option<usize>,
}

impl JanusRetryPolicy {
    pub fn steps_for_attempt(self, base_steps: usize, attempt: usize) -> usize {
        let retry_index = attempt.saturating_sub(1);
        let multiplier = 1usize
            .saturating_add(retry_index.saturating_mul(self.additional_step_multiples_per_retry));
        let adjusted = base_steps.saturating_mul(multiplier.max(1));
        self.max_steps
            .map(|limit| adjusted.min(limit))
            .unwrap_or(adjusted)
    }
}

impl Default for JanusRetryPolicy {
    fn default() -> Self {
        Self {
            additional_step_multiples_per_retry: 1,
            max_steps: None,
        }
    }
}

#[derive(Clone)]
enum RegisteredBackend {
    Fixed(Arc<dyn ExternalEvaluator>),
    AdaptiveJanus {
        config: JanusMaceConfig,
        timeout: Option<Duration>,
        retry_policy: JanusRetryPolicy,
    },
}

/// Stage-aware executor that routes each Scott stage to its configured backend adapter.
#[derive(Clone, Default)]
pub struct RoutedBackendStageExecutor {
    backends: IndexMap<ExternalProgram, RegisteredBackend>,
    routing: Option<ScottBackendRoutingPolicy>,
}

impl RoutedBackendStageExecutor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_backend(
        mut self,
        backend_mode: ScottBackendMode,
        backend: Arc<dyn BackendEvaluator>,
    ) -> Self {
        let program = ExternalProgram::from(backend_mode);
        self.backends.insert(
            program,
            RegisteredBackend::Fixed(Arc::new(BackendEvaluatorExternalAdapter::new(
                program, backend,
            ))),
        );
        self
    }

    pub fn with_external_evaluator(
        mut self,
        program: ExternalProgram,
        backend: Arc<dyn ExternalEvaluator>,
    ) -> Self {
        self.backends
            .insert(program, RegisteredBackend::Fixed(backend));
        self
    }

    pub fn with_adaptive_janus_backend(
        mut self,
        config: JanusMaceConfig,
        timeout: Option<Duration>,
        retry_policy: JanusRetryPolicy,
    ) -> Self {
        self.backends.insert(
            ExternalProgram::JanusMace,
            RegisteredBackend::AdaptiveJanus {
                config,
                timeout,
                retry_policy,
            },
        );
        self
    }

    pub fn with_routing_policy(mut self, routing: ScottBackendRoutingPolicy) -> Self {
        self.routing = Some(routing);
        self
    }
}

impl std::fmt::Debug for RoutedBackendStageExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RoutedBackendStageExecutor")
            .field(
                "registered_backends",
                &self.backends.keys().collect::<Vec<_>>(),
            )
            .field("routing", &self.routing)
            .finish()
    }
}

impl BackendStageExecutor for RoutedBackendStageExecutor {
    fn evaluate_stage(
        &self,
        plan: &ScottProcedurePlan,
        request: &ScottProcedureRequest,
        ticket: &StageTicket,
    ) -> Result<patina_types::EvalResult, EvalError> {
        let program = self.external_program_for_stage(plan, ticket)?;
        let backend = self
            .backends
            .get(&program)
            .ok_or_else(|| EvalError::ProcessFailed {
                exit_code: None,
                stderr: format!(
                    "no external adapter registered for Scott stage {} program `{}`",
                    ticket.stage.get(),
                    program.as_str()
                ),
            })?;

        let stage_workdir = stage_attempt_dir(ticket);
        fs::create_dir_all(&stage_workdir).map_err(EvalError::IoError)?;
        let external_request = external_evaluation_request(plan, request, ticket, program);
        match backend {
            RegisteredBackend::Fixed(backend) => external_outcome_to_eval_result(
                program,
                backend
                    .evaluate(&external_request)
                    .map_err(external_to_eval_error)?,
            ),
            RegisteredBackend::AdaptiveJanus {
                config,
                timeout,
                retry_policy,
            } => {
                let mut adjusted_config = config.clone();
                adjusted_config.steps =
                    retry_policy.steps_for_attempt(adjusted_config.steps, ticket.attempt);
                let backend = BackendEvaluatorExternalAdapter::new(
                    ExternalProgram::JanusMace,
                    JanusMaceBackend::new(adjusted_config, *timeout),
                );
                external_outcome_to_eval_result(
                    program,
                    backend
                        .evaluate(&external_request)
                        .map_err(external_to_eval_error)?,
                )
            }
        }
    }

    fn external_program_for_stage(
        &self,
        plan: &ScottProcedurePlan,
        ticket: &StageTicket,
    ) -> Result<ExternalProgram, EvalError> {
        let stage_program = stage_engine_external_program(plan, ticket)?;
        if let Some(override_backend) = self
            .routing
            .as_ref()
            .and_then(|routing| routing.backend_override_for_stage(ticket))
        {
            return Ok(ExternalProgram::from(override_backend));
        }

        if stage_program == ExternalProgram::Gulp {
            let backend_mode = self
                .routing
                .as_ref()
                .map(|routing| routing.default_backend)
                .unwrap_or(plan.evaluator.settings.backend_mode);
            return Ok(ExternalProgram::from(backend_mode));
        }

        Ok(stage_program)
    }
}

#[derive(Debug, Error)]
pub enum LocalProcedureError {
    #[error("invalid Scott procedure state: {0}")]
    Procedure(#[from] ScottEvaluatorError),
    #[error("local runner encountered unsupported procedure action: {0:?}")]
    UnsupportedAction(ProcedureAction),
}

/// Drives a full Scott procedure locally using a blocking backend-stage executor.
pub fn run_local_procedure(
    plan: &ScottProcedurePlan,
    request: &ScottProcedureRequest,
    executor: &dyn BackendStageExecutor,
) -> Result<ScottEvalOutcome, LocalProcedureError> {
    validate(plan, request)?;
    let mut state = ProcedureState::new(plan, request)?;

    loop {
        match state.next_action() {
            ProcedureAction::Submit(ticket) | ProcedureAction::Retry(ticket) => {
                let stage_request = ScottProcedureRequest {
                    candidate: state.current_candidate().clone(),
                    request_id: request.request_id.clone(),
                    workdir: request.workdir.clone(),
                };
                let report = ghost_result_to_attempt_report(
                    GhostAttemptContext {
                        stage: ticket.stage,
                        attempt: ticket.attempt,
                        primary_output_path: executor
                            .primary_output_path(plan, &ticket)
                            .ok()
                            .map(|path| path.display().to_string()),
                    },
                    executor.evaluate_stage(plan, &stage_request, &ticket),
                );
                let _ = state.apply_attempt_report(plan, report)?;
            }
            ProcedureAction::Retrieve(_) => {
                let _ = state.complete_retrieval(plan)?;
            }
            ProcedureAction::AdvanceTo(_) => {}
            ProcedureAction::FinishAccepted
            | ProcedureAction::FinishRejected
            | ProcedureAction::FinishFailed => return Ok(state.into_outcome()),
            other => return Err(LocalProcedureError::UnsupportedAction(other)),
        }
    }
}

fn validate(
    plan: &ScottProcedurePlan,
    request: &ScottProcedureRequest,
) -> Result<(), ScottEvaluatorError> {
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
    Ok(())
}

fn stage_attempt_dir(ticket: &StageTicket) -> PathBuf {
    let mut path = PathBuf::from(ticket.workdir.as_str());
    path.push(format!(
        "stage_{:02}_attempt_{:02}",
        ticket.stage.get(),
        ticket.attempt
    ));
    path
}

fn default_stage_primary_output_path_for_program(
    program: ExternalProgram,
    ticket: &StageTicket,
) -> PathBuf {
    let stage_workdir = stage_attempt_dir(ticket);
    match program {
        ExternalProgram::Gulp => stage_workdir.join("gulp_klmc.gout"),
        ExternalProgram::JanusMace => stage_workdir.join("janus_result.json"),
        ExternalProgram::Cp2k => stage_workdir.join("cp2k.out"),
        ExternalProgram::Crystal => stage_workdir.join("crystal.out"),
        ExternalProgram::Aims => stage_workdir.join("aims.out"),
        ExternalProgram::Vasp => stage_workdir.join("OUTCAR"),
        ExternalProgram::Nwchem => stage_workdir.join("output.nw"),
        ExternalProgram::Dmol => stage_workdir.join("outmol"),
        ExternalProgram::NoEvalExport => stage_workdir.join("export"),
    }
}

fn stage_engine_external_program(
    plan: &ScottProcedurePlan,
    ticket: &StageTicket,
) -> Result<ExternalProgram, EvalError> {
    let stage_plan = plan
        .stages
        .get(ticket.stage)
        .ok_or_else(|| EvalError::ParseFailed {
            line: 0,
            reason: format!("unknown Scott stage {}", ticket.stage.get()),
        })?;
    Ok(ExternalProgram::from(stage_plan.engine))
}

fn external_evaluation_request(
    plan: &ScottProcedurePlan,
    request: &ScottProcedureRequest,
    ticket: &StageTicket,
    program: ExternalProgram,
) -> ExternalEvaluationRequest {
    ExternalEvaluationRequest {
        request_id: ticket.request_id.clone(),
        stage: ticket.stage.get(),
        program,
        mode: if program == ExternalProgram::NoEvalExport {
            ExternalEvaluationMode::ExportOnly
        } else {
            ExternalEvaluationMode::Relaxation
        },
        execution: ExternalExecutionMode::LocalSubprocess,
        retrieve_relaxed_geometry: plan.evaluator.settings.retrieve_relaxed_geometry,
        candidate: request.candidate.clone(),
        workdir: stage_attempt_dir(ticket),
        templates: ExternalTemplateSet::default(),
    }
}

fn external_outcome_to_eval_result(
    program: ExternalProgram,
    outcome: patina_external::ExternalEvaluationOutcome,
) -> Result<patina_types::EvalResult, EvalError> {
    match outcome.status {
        ExternalEvaluationStatus::Converged => {
            outcome.result.ok_or_else(|| EvalError::ParseFailed {
                line: 0,
                reason: format!(
                    "external adapter `{}` converged without an EvalResult payload",
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
        ExternalEvaluationStatus::ExportedOnly => Err(EvalError::ProcessFailed {
            exit_code: None,
            stderr: format!(
                "external adapter `{}` exported inputs without evaluating energy",
                program.as_str()
            ),
        }),
        ExternalEvaluationStatus::Submitted => Err(EvalError::ProcessFailed {
            exit_code: None,
            stderr: format!(
                "external adapter `{}` submitted a deferred scheduler job; local runner expects terminal local execution",
                program.as_str()
            ),
        }),
    }
}

fn external_to_eval_error(error: ExternalError) -> EvalError {
    match error {
        ExternalError::InvalidRequest { reason } => EvalError::ParseFailed { line: 0, reason },
        ExternalError::UnsupportedCandidate { program, reason } => EvalError::ProcessFailed {
            exit_code: None,
            stderr: format!("{} unsupported candidate: {reason}", program.as_str()),
        },
        ExternalError::UnsupportedProgram { program, reason } => EvalError::ProcessFailed {
            exit_code: None,
            stderr: format!("{} unsupported program: {reason}", program.as_str()),
        },
        ExternalError::ProcessFailed {
            exit_code, stderr, ..
        } => EvalError::ProcessFailed { exit_code, stderr },
        ExternalError::ParseFailed { reason, .. } => EvalError::ParseFailed { line: 0, reason },
        ExternalError::NotConverged {
            energy,
            n_steps,
            partial_result,
            ..
        } => EvalError::NotConverged {
            energy,
            n_steps,
            partial_result,
        },
        ExternalError::Timeout { elapsed, .. } => EvalError::Timeout { elapsed },
        ExternalError::TemplateInvalid { path, reason, .. } => {
            EvalError::TemplateInvalid { path, reason }
        }
        ExternalError::Io(error) => EvalError::IoError(error),
    }
}

#[cfg(test)]
mod tests {
    use super::BackendStageExecutor;
    use super::{run_local_procedure, FixedBackendStageExecutor};
    use super::{JanusRetryPolicy, RoutedBackendStageExecutor, ScottBackendRoutingPolicy};
    use patina_evaluator::{
        FinalStageFailurePolicy, ScottBackendMode, ScottEvaluatorPlan, ScottEvaluatorSettings,
        ScottLatticeMode, ScottProcedureIntent, ScottProcedurePlan, ScottProcedureRequest,
        StageEngine, StageIndex, StagePlan, StageSelection,
    };
    use patina_external::{ExternalProgram, MockBackend};
    use patina_types::{Candidate, EvalResult};
    use std::sync::Arc;
    use std::sync::Mutex;
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
            workdir: "/tmp/scott-local-runner".into(),
        }
    }

    fn single_stage_plan() -> ScottProcedurePlan {
        ScottProcedurePlan {
            evaluator: ScottEvaluatorPlan {
                master_template: "/tmp/Master.gin".into(),
                atoms_in: None,
                work_root: "/tmp".into(),
                settings: ScottEvaluatorSettings {
                    backend_mode: ScottBackendMode::Gulp,
                    procedure_intent: ScottProcedureIntent::SingleEvaluation,
                    lattice_mode: ScottLatticeMode::Cluster,
                    max_relaxation_attempts: 1,
                    gnorm_tolerance: 1.0e-4,
                    final_stage_failure_policy: FinalStageFailurePolicy::RejectCandidate,
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
        }
    }

    #[test]
    fn janus_retry_policy_increases_step_budget_by_attempt() {
        let policy = JanusRetryPolicy {
            additional_step_multiples_per_retry: 1,
            max_steps: Some(240),
        };

        assert_eq!(policy.steps_for_attempt(80, 1), 80);
        assert_eq!(policy.steps_for_attempt(80, 2), 160);
        assert_eq!(policy.steps_for_attempt(80, 3), 240);
        assert_eq!(policy.steps_for_attempt(80, 4), 240);
    }

    fn two_stage_plan() -> ScottProcedurePlan {
        ScottProcedurePlan {
            evaluator: ScottEvaluatorPlan {
                master_template: "/tmp/Master.gin".into(),
                atoms_in: None,
                work_root: "/tmp".into(),
                settings: ScottEvaluatorSettings {
                    backend_mode: ScottBackendMode::Gulp,
                    procedure_intent: ScottProcedureIntent::ProductionRun,
                    lattice_mode: ScottLatticeMode::Cluster,
                    max_relaxation_attempts: 1,
                    gnorm_tolerance: 1.0e-4,
                    final_stage_failure_policy: FinalStageFailurePolicy::RejectCandidate,
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
    fn local_runner_executes_single_stage_with_mock_backend() {
        let executor = FixedBackendStageExecutor::new(Arc::new(MockBackend));
        let outcome = run_local_procedure(&single_stage_plan(), &request(), &executor).unwrap();
        assert!(outcome.final_result.is_some());
        assert_eq!(outcome.final_stage, Some(StageIndex(1)));
        assert!(!outcome.relax_failed);
        assert_eq!(outcome.provenance.completed_stages.len(), 1);
    }

    #[test]
    fn routed_executor_can_run_mixed_stage_plan() {
        let executor = RoutedBackendStageExecutor::new()
            .with_backend(ScottBackendMode::Gulp, Arc::new(MockBackend))
            .with_backend(ScottBackendMode::JanusMace, Arc::new(MockBackend))
            .with_routing_policy(
                ScottBackendRoutingPolicy::from_plan(&two_stage_plan())
                    .with_stage_backend(1, ScottBackendMode::JanusMace)
                    .with_stage_backend(2, ScottBackendMode::Gulp),
            );
        let outcome = run_local_procedure(&two_stage_plan(), &request(), &executor).unwrap();
        assert!(outcome.final_result.is_some());
        assert_eq!(outcome.final_stage, Some(StageIndex(2)));
        assert_eq!(outcome.provenance.completed_stages.len(), 2);
    }

    #[test]
    fn routed_executor_uses_stage_engine_when_no_backend_override_exists() {
        let executor = RoutedBackendStageExecutor::new()
            .with_backend(ScottBackendMode::Gulp, Arc::new(MockBackend))
            .with_routing_policy(ScottBackendRoutingPolicy::from_plan(&two_stage_plan()));
        let ticket = patina_evaluator::StageTicket {
            request_id: "req-1".into(),
            backend_mode: ScottBackendMode::Gulp,
            stage: StageIndex(2),
            attempt: 1,
            workdir: "/tmp/scott-local-runner".into(),
        };

        assert_eq!(
            executor
                .external_program_for_stage(&two_stage_plan(), &ticket)
                .expect("program"),
            ExternalProgram::Aims
        );
        let err = executor
            .evaluate_stage(&two_stage_plan(), &request(), &ticket)
            .expect_err("AIMS stage should require an AIMS adapter");
        assert!(matches!(
            err,
            patina_external::EvalError::ProcessFailed { stderr, .. }
                if stderr.contains("aims")
        ));
    }

    #[test]
    fn routed_executor_default_backend_can_substitute_gulp_stage() {
        let mut policy = ScottBackendRoutingPolicy::from_plan(&single_stage_plan());
        policy.default_backend = ScottBackendMode::JanusMace;
        let executor = RoutedBackendStageExecutor::new()
            .with_backend(ScottBackendMode::JanusMace, Arc::new(MockBackend))
            .with_routing_policy(policy);

        let outcome = run_local_procedure(&single_stage_plan(), &request(), &executor).unwrap();

        assert!(outcome.final_result.is_some());
        assert_eq!(outcome.final_stage, Some(StageIndex(1)));
    }

    #[derive(Debug, Default)]
    struct CarryForwardExecutor {
        seen_labels: Mutex<Vec<String>>,
    }

    impl BackendStageExecutor for CarryForwardExecutor {
        fn evaluate_stage(
            &self,
            _plan: &ScottProcedurePlan,
            request: &ScottProcedureRequest,
            ticket: &patina_evaluator::StageTicket,
        ) -> Result<EvalResult, patina_external::EvalError> {
            self.seen_labels.lock().expect("lock").push(format!(
                "stage{}:{}",
                ticket.stage.get(),
                request.candidate.label
            ));
            let next_label = if ticket.stage == StageIndex(1) {
                "after-stage1".to_string()
            } else {
                format!("final-from-{}", request.candidate.label)
            };
            let mut relaxed = request.candidate.clone();
            relaxed.label = next_label;
            Ok(EvalResult {
                energy: -(ticket.stage.get() as f64),
                forces: vec![[0.0, 0.0, 0.0]; relaxed.len()],
                relaxed_candidate: relaxed,
                converged: true,
                wall_time: Duration::from_secs(1),
            })
        }
    }

    #[test]
    fn local_runner_carries_retrieved_geometry_between_stages() {
        let executor = CarryForwardExecutor::default();
        let outcome = run_local_procedure(&two_stage_plan(), &request(), &executor).unwrap();
        let seen = executor.seen_labels.lock().expect("lock").clone();
        assert_eq!(seen, vec!["stage1:mg1", "stage2:after-stage1"]);
        assert_eq!(
            outcome
                .final_result
                .as_ref()
                .map(|result| result.relaxed_candidate.label.as_str()),
            Some("final-from-after-stage1")
        );
    }

    #[test]
    fn routing_policy_defaults_and_overrides_per_stage() {
        let policy = ScottBackendRoutingPolicy::from_plan(&two_stage_plan())
            .with_stage_backend(1, ScottBackendMode::JanusMace);
        assert_eq!(
            policy.backend_for_stage(&patina_evaluator::StageTicket {
                request_id: "req".into(),
                backend_mode: ScottBackendMode::Gulp,
                stage: StageIndex(1),
                attempt: 1,
                workdir: "/tmp".into(),
            }),
            ScottBackendMode::JanusMace
        );
        assert_eq!(
            policy.backend_for_stage(&patina_evaluator::StageTicket {
                request_id: "req".into(),
                backend_mode: ScottBackendMode::Gulp,
                stage: StageIndex(2),
                attempt: 1,
                workdir: "/tmp".into(),
            }),
            ScottBackendMode::Gulp
        );
    }
}
