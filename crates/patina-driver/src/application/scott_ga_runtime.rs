use anyhow::{anyhow, Context, Result};
use camino::Utf8PathBuf;
use patina_evaluator::{ScottProcedurePlan, ScottProcedureRequest, ScottStateDigest};
use patina_runner::PersistentWorkerPool;
use patina_runtime::{run_local_procedure, LocalProcedureError, RoutedBackendStageExecutor};
use patina_search::{
    DuplicatePolicy, ScottGaMember, ScottGaOrigin, ScottGaPendingRepopulation,
    ScottParityGeneticAlgorithm, WorkflowLineage,
};
use patina_types::{
    Candidate, EvalResult, WorkerFailure, WorkerFailureKind, WorkerOutcome, WorkerRequest,
    WorkerResponse,
};
use rayon::prelude::*;
use serde::Serialize;
use std::fs;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::ga_execution::{RustJanusGenerationArtifact, RustJanusOriginMetricRow};
use super::ports::GaEvaluationPort;

pub struct PersistentPoolGaEvaluator<'a> {
    pool: &'a PersistentWorkerPool,
}

impl<'a> PersistentPoolGaEvaluator<'a> {
    pub fn new(pool: &'a PersistentWorkerPool) -> Self {
        Self { pool }
    }
}

impl GaEvaluationPort for PersistentPoolGaEvaluator<'_> {
    fn evaluate_generation(&self, requests: &[WorkerRequest]) -> Result<Vec<WorkerResponse>> {
        self.pool.evaluate_requests(requests).map_err(Into::into)
    }
}

#[derive(Debug, Clone)]
pub struct StagedScottGaEvaluator {
    executor: RoutedBackendStageExecutor,
    plan: ScottProcedurePlan,
    work_root: Utf8PathBuf,
    procedure_traces: Arc<Mutex<Vec<StagedScottProcedureTraceRecord>>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StagedScottProcedureTraceStatus {
    Accepted,
    NoAcceptedResult,
    InconsistentState,
    ProcedureError,
}

#[derive(Debug, Clone, Serialize)]
pub struct StagedScottProcedureTraceRecord {
    pub request_id: String,
    pub generation: Option<usize>,
    pub request_workdir: String,
    pub input_label: String,
    pub status: StagedScottProcedureTraceStatus,
    pub procedure_state_digest: Option<ScottStateDigest>,
    pub failure_message: Option<String>,
}

impl StagedScottGaEvaluator {
    pub fn new(
        executor: RoutedBackendStageExecutor,
        plan: ScottProcedurePlan,
        work_root: Utf8PathBuf,
    ) -> Self {
        Self {
            executor,
            plan,
            work_root,
            procedure_traces: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn procedure_traces(&self) -> Vec<StagedScottProcedureTraceRecord> {
        match self.procedure_traces.lock() {
            Ok(traces) => traces.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    pub fn shared_procedure_traces(&self) -> Arc<Mutex<Vec<StagedScottProcedureTraceRecord>>> {
        Arc::clone(&self.procedure_traces)
    }
}

impl GaEvaluationPort for StagedScottGaEvaluator {
    fn evaluate_generation(&self, requests: &[WorkerRequest]) -> Result<Vec<WorkerResponse>> {
        fs::create_dir_all(self.work_root.as_std_path()).with_context(|| {
            format!("failed to create staged GA work root `{}`", self.work_root)
        })?;

        requests
            .par_iter()
            .enumerate()
            .map(|(index, request)| {
                let generation = request.generation.unwrap_or(0);
                let request_workdir = self.work_root.join(format!(
                    "generation_{generation:04}/request_{index:04}_{}",
                    request.request_id
                ));
                fs::create_dir_all(request_workdir.as_std_path()).with_context(|| {
                    format!("failed to create staged Scott GA request workdir `{request_workdir}`")
                })?;

                let procedure_request = ScottProcedureRequest {
                    candidate: request.candidate.clone(),
                    request_id: request.request_id.clone(),
                    workdir: request_workdir,
                };
                let outcome = run_local_procedure(&self.plan, &procedure_request, &self.executor);
                let trace_record =
                    build_staged_trace_record(request, &procedure_request.workdir, &outcome);
                match self.procedure_traces.lock() {
                    Ok(mut traces) => traces.push(trace_record),
                    Err(poisoned) => poisoned.into_inner().push(trace_record),
                }

                Ok(WorkerResponse {
                    request_id: request.request_id.clone(),
                    generation: request.generation,
                    worker_slot: None,
                    outcome: map_staged_outcome(outcome),
                })
            })
            .collect()
    }
}

fn build_staged_trace_record(
    request: &WorkerRequest,
    request_workdir: &Utf8PathBuf,
    outcome: &Result<patina_evaluator::ScottEvalOutcome, LocalProcedureError>,
) -> StagedScottProcedureTraceRecord {
    match outcome {
        Ok(outcome) => {
            let consistency_error = outcome.validate_consistency().err();
            let status = if consistency_error.is_some() {
                StagedScottProcedureTraceStatus::InconsistentState
            } else if outcome.final_result.is_some() {
                StagedScottProcedureTraceStatus::Accepted
            } else {
                StagedScottProcedureTraceStatus::NoAcceptedResult
            };
            let failure_message = consistency_error
                .map(|error| error.to_string())
                .or_else(|| outcome.last_failure_message().map(ToOwned::to_owned));

            StagedScottProcedureTraceRecord {
                request_id: request.request_id.clone(),
                generation: request.generation,
                request_workdir: request_workdir.to_string(),
                input_label: request.candidate.label.clone(),
                status,
                procedure_state_digest: Some(outcome.state_digest()),
                failure_message,
            }
        }
        Err(error) => StagedScottProcedureTraceRecord {
            request_id: request.request_id.clone(),
            generation: request.generation,
            request_workdir: request_workdir.to_string(),
            input_label: request.candidate.label.clone(),
            status: StagedScottProcedureTraceStatus::ProcedureError,
            procedure_state_digest: None,
            failure_message: Some(error.to_string()),
        },
    }
}

pub fn build_worker_requests(generation: usize, candidates: &[Candidate]) -> Vec<WorkerRequest> {
    candidates
        .iter()
        .enumerate()
        .map(|(idx, candidate)| WorkerRequest {
            request_id: format!("gen_{generation:04}_candidate_{idx:04}"),
            generation: Some(generation),
            candidate: candidate.clone(),
        })
        .collect()
}

pub fn pair_initial_responses(
    candidates: &[Candidate],
    responses: &[WorkerResponse],
) -> Result<Vec<(Candidate, EvalResult, ScottGaOrigin, WorkflowLineage)>> {
    ensure_matching_batch_sizes("initial_population", candidates.len(), responses.len())?;
    candidates
        .iter()
        .cloned()
        .zip(responses.iter())
        .map(|(candidate, response)| {
            Ok((
                candidate.clone(),
                response_to_eval_result(&candidate, response),
                ScottGaOrigin::Seed,
                WorkflowLineage::seed(candidate.label.clone()).with_generation(0),
            ))
        })
        .collect()
}

pub fn pair_child_responses(
    children: Vec<(Candidate, ScottGaOrigin, WorkflowLineage)>,
    responses: &[WorkerResponse],
) -> Result<Vec<(Candidate, EvalResult, ScottGaOrigin, WorkflowLineage)>> {
    ensure_matching_batch_sizes("child_generation", children.len(), responses.len())?;
    children
        .into_iter()
        .zip(responses.iter())
        .map(|((candidate, origin, lineage), response)| {
            Ok((
                candidate.clone(),
                response_to_eval_result(&candidate, response),
                origin,
                lineage,
            ))
        })
        .collect()
}

fn ensure_matching_batch_sizes(context: &str, expected: usize, actual: usize) -> Result<()> {
    if expected != actual {
        return Err(anyhow!(
            "GA batch mismatch in `{context}`: expected {expected} responses but received {actual}"
        ));
    }
    Ok(())
}

pub fn resolve_pending_repopulation<D: DuplicatePolicy>(
    generation: usize,
    evaluator: &impl GaEvaluationPort,
    controller: &mut ScottParityGeneticAlgorithm<D>,
    population: &mut Vec<ScottGaMember>,
    responses: &mut Vec<WorkerResponse>,
    response_origins: &mut Vec<ScottGaOrigin>,
    elapsed_secs: &mut f64,
) -> Result<()> {
    for pass in 0..crate::MAX_REPOPULATION_EVAL_PASSES {
        let pending = controller.pending_repopulation(population);
        if pending.is_empty() {
            break;
        }

        let repop_requests = build_repopulation_requests(generation, pass, &pending);
        let started = std::time::Instant::now();
        let repop_responses = evaluator.evaluate_generation(&repop_requests)?;
        *elapsed_secs += started.elapsed().as_secs_f64();
        let evaluated = pair_repopulation_responses(&pending, &repop_responses);
        response_origins.extend(pending.iter().map(|entry| entry.origin));
        let _ = controller.absorb_repopulation_results(generation, population, evaluated)?;
        responses.extend(repop_responses);
    }
    Ok(())
}

pub fn build_rust_janus_generation_artifact(
    generation: usize,
    phase: &str,
    elapsed_secs: f64,
    responses: &[WorkerResponse],
    boundary_population: &[ScottGaMember],
    working_population: &[ScottGaMember],
    trace: Option<&patina_search::ScottGaGenerationTrace>,
) -> RustJanusGenerationArtifact {
    let mut energies = Vec::new();
    let mut converged_count = 0usize;
    let mut success_count = 0usize;
    let mut failure_count = 0usize;
    for response in responses {
        match &response.outcome {
            WorkerOutcome::Success { result } => {
                success_count += 1;
                energies.push(result.energy);
                if result.converged {
                    converged_count += 1;
                }
            }
            WorkerOutcome::Failure { .. } => {
                failure_count += 1;
            }
        }
    }
    let (best_energy, mean_energy, worst_energy) =
        crate::application::report_types::summarize_energy_stats(&energies);
    let failure_kind_counts = crate::application::report_types::summarize_failure_kinds(responses);

    RustJanusGenerationArtifact {
        generation,
        phase: phase.into(),
        request_count: responses.len(),
        success_count,
        failure_count,
        failure_kind_counts,
        converged_count,
        elapsed_secs,
        best_energy,
        mean_energy,
        worst_energy,
        boundary_population_size: boundary_population.len(),
        boundary_valid_population_size: boundary_population
            .iter()
            .filter(|member| member.is_valid())
            .count(),
        population_size: working_population.len(),
        valid_population_size: working_population
            .iter()
            .filter(|member| member.is_valid())
            .count(),
        duplicate_count: trace.map(|value| value.duplicate_count).unwrap_or(0),
        duplicate_hashkey_count: trace
            .map(|value| value.duplicate_hashkey_count)
            .unwrap_or(0),
        duplicate_pmoi_count: trace.map(|value| value.duplicate_pmoi_count).unwrap_or(0),
        duplicate_energy_tol_count: trace
            .map(|value| value.duplicate_energy_tol_count)
            .unwrap_or(0),
        repopulated_count: trace.map(|value| value.repopulated_count).unwrap_or(0),
    }
}

pub fn build_rust_janus_origin_metrics(
    generation: usize,
    phase: &str,
    response_origins: &[ScottGaOrigin],
    responses: &[WorkerResponse],
    boundary_population: &[ScottGaMember],
    working_population: &[ScottGaMember],
) -> Vec<RustJanusOriginMetricRow> {
    scott_ga_origins()
        .into_iter()
        .map(|origin| {
            let mut request_count = 0usize;
            let mut success_count = 0usize;
            let mut converged_count = 0usize;
            for (response, response_origin) in responses.iter().zip(response_origins.iter()) {
                if *response_origin != origin {
                    continue;
                }
                request_count += 1;
                if let WorkerOutcome::Success { result } = &response.outcome {
                    success_count += 1;
                    if result.converged {
                        converged_count += 1;
                    }
                }
            }
            let survivor_count = boundary_population
                .iter()
                .filter(|member| member.origin == origin)
                .count();
            let working_survivor_count = working_population
                .iter()
                .filter(|member| member.origin == origin)
                .count();
            RustJanusOriginMetricRow {
                generation,
                phase: phase.to_string(),
                origin: origin.as_str().to_string(),
                request_count,
                success_count,
                converged_count,
                survivor_count,
                working_survivor_count,
            }
        })
        .collect()
}

fn build_repopulation_requests(
    generation: usize,
    pass: usize,
    pending: &[ScottGaPendingRepopulation],
) -> Vec<WorkerRequest> {
    pending
        .iter()
        .map(|entry| WorkerRequest {
            request_id: format!(
                "gen_{generation:04}_repop_pass_{pass:02}_slot_{:04}",
                entry.slot_index
            ),
            generation: Some(generation),
            candidate: entry.candidate.clone(),
        })
        .collect()
}

fn pair_repopulation_responses(
    pending: &[ScottGaPendingRepopulation],
    responses: &[WorkerResponse],
) -> Vec<(usize, Candidate, EvalResult, ScottGaOrigin, WorkflowLineage)> {
    pending
        .iter()
        .zip(responses.iter())
        .map(|(entry, response)| {
            (
                entry.slot_index,
                entry.candidate.clone(),
                response_to_eval_result(&entry.candidate, response),
                entry.origin,
                entry.lineage.clone(),
            )
        })
        .collect()
}

fn response_to_eval_result(candidate: &Candidate, response: &WorkerResponse) -> EvalResult {
    match &response.outcome {
        WorkerOutcome::Success { result } => result.clone(),
        WorkerOutcome::Failure { .. } => EvalResult {
            energy: f64::INFINITY,
            forces: vec![[0.0, 0.0, 0.0]; candidate.len()],
            relaxed_candidate: candidate.clone(),
            converged: false,
            wall_time: Duration::from_secs(0),
        },
    }
}

fn scott_ga_origins() -> [ScottGaOrigin; 6] {
    [
        ScottGaOrigin::Seed,
        ScottGaOrigin::Crosso,
        ScottGaOrigin::Mutate,
        ScottGaOrigin::Mutcrs,
        ScottGaOrigin::RePopM,
        ScottGaOrigin::RePopR,
    ]
}

fn map_staged_outcome(
    outcome: Result<patina_evaluator::ScottEvalOutcome, LocalProcedureError>,
) -> WorkerOutcome {
    match outcome {
        Ok(outcome) => {
            if let Err(error) = outcome.validate_consistency() {
                return WorkerOutcome::Failure {
                    error: WorkerFailure {
                        kind: WorkerFailureKind::Internal,
                        message: format!(
                            "Scott staged procedure returned inconsistent evaluation state: {error}"
                        ),
                    },
                };
            }

            match outcome.final_result {
                Some(result) => WorkerOutcome::Success { result },
                None => WorkerOutcome::Failure {
                    error: map_scott_outcome_failure(&outcome),
                },
            }
        }
        Err(error) => WorkerOutcome::Failure {
            error: map_local_procedure_error(&error),
        },
    }
}

fn map_scott_outcome_failure(outcome: &patina_evaluator::ScottEvalOutcome) -> WorkerFailure {
    if let Some(failure) = outcome.state.failures.last() {
        return WorkerFailure {
            kind: map_stage_backend_status_to_failure_kind(
                failure.backend_status,
                &failure.message,
            ),
            message: format!(
                "Scott staged procedure produced no accepted final result; stage {} attempt {}: {}",
                failure.stage.get(),
                failure.attempt,
                failure.message
            ),
        };
    }

    let message = outcome
        .last_procedure_gate_message()
        .map(|detail| {
            format!(
                "Scott staged procedure produced no accepted final result; last procedure gate: {detail}"
            )
        })
        .or_else(|| {
            outcome.last_failure_message().map(|detail| {
                format!(
                    "Scott staged procedure produced no accepted final result; last failure: {detail}"
                )
            })
        })
        .unwrap_or_else(|| "Scott staged procedure produced no accepted final result".into());

    WorkerFailure {
        kind: WorkerFailureKind::NotConverged,
        message,
    }
}

fn map_stage_backend_status_to_failure_kind(
    status: patina_evaluator::StageBackendStatus,
    message: &str,
) -> WorkerFailureKind {
    match status {
        patina_evaluator::StageBackendStatus::InvalidEnergy => WorkerFailureKind::ParseFailed,
        patina_evaluator::StageBackendStatus::RequiresMoreCycles
        | patina_evaluator::StageBackendStatus::RejectedByProcedure => {
            WorkerFailureKind::NotConverged
        }
        patina_evaluator::StageBackendStatus::Crashed => {
            let lower = message.to_ascii_lowercase();
            if lower.contains("template") {
                WorkerFailureKind::TemplateInvalid
            } else if lower.contains("timed out") {
                WorkerFailureKind::Timeout
            } else if lower.contains("i/o") {
                WorkerFailureKind::Io
            } else {
                WorkerFailureKind::ProcessFailed
            }
        }
        patina_evaluator::StageBackendStatus::Converged
        | patina_evaluator::StageBackendStatus::ConvergedWithGradientWarning => {
            WorkerFailureKind::Internal
        }
    }
}

fn map_local_procedure_error(error: &LocalProcedureError) -> WorkerFailure {
    match error {
        LocalProcedureError::Procedure(inner) => WorkerFailure {
            kind: WorkerFailureKind::Internal,
            message: format!("Scott procedure state error: {inner}"),
        },
        LocalProcedureError::UnsupportedAction(action) => WorkerFailure {
            kind: WorkerFailureKind::Internal,
            message: format!("Scott local runner does not support action `{action:?}`"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{build_rust_janus_origin_metrics, pair_child_responses, pair_initial_responses};
    use patina_search::{ScottGaMember, ScottGaOrigin, WorkflowLineage};
    use patina_types::{Candidate, EvalResult, WorkerOutcome, WorkerResponse};
    use std::time::Duration;

    fn candidate(label: &str) -> Candidate {
        Candidate::cluster(label, vec!["Mg".into()], vec![[0.0, 0.0, 0.0]])
    }

    fn success_response(label: &str) -> WorkerResponse {
        WorkerResponse {
            request_id: label.into(),
            generation: Some(0),
            worker_slot: Some(0),
            outcome: WorkerOutcome::Success {
                result: EvalResult {
                    energy: -1.0,
                    forces: vec![[0.0, 0.0, 0.0]],
                    relaxed_candidate: candidate(label),
                    converged: true,
                    wall_time: Duration::from_secs(0),
                },
            },
        }
    }

    #[test]
    fn pair_initial_responses_rejects_mismatched_batch_sizes() {
        let candidates = vec![candidate("a"), candidate("b")];
        let responses = vec![success_response("a")];

        let error = pair_initial_responses(&candidates, &responses).expect_err("mismatch");
        assert!(error
            .to_string()
            .contains("GA batch mismatch in `initial_population`"));
    }

    #[test]
    fn pair_child_responses_rejects_mismatched_batch_sizes() {
        let children = vec![
            (
                candidate("a"),
                ScottGaOrigin::Crosso,
                WorkflowLineage::seed("a").with_generation(1),
            ),
            (
                candidate("b"),
                ScottGaOrigin::Mutate,
                WorkflowLineage::seed("b").with_generation(1),
            ),
        ];
        let responses = vec![success_response("a")];

        let error = pair_child_responses(children, &responses).expect_err("mismatch");
        assert!(error
            .to_string()
            .contains("GA batch mismatch in `child_generation`"));
    }

    fn member(label: &str, origin: ScottGaOrigin, converged: bool) -> ScottGaMember {
        ScottGaMember {
            source_candidate: candidate(&format!("{label}_source")),
            result: EvalResult {
                energy: if converged { -1.0 } else { f64::INFINITY },
                forces: vec![[0.0, 0.0, 0.0]],
                relaxed_candidate: candidate(label),
                converged,
                wall_time: Duration::from_secs(0),
            },
            origin,
            occurrences: 1,
            lineage: WorkflowLineage::seed(format!("{label}_source")),
            topology: patina_search::ScottGaTopologyIdentity::default(),
        }
    }

    #[test]
    fn origin_metrics_separate_boundary_and_working_survivors() {
        let responses = vec![success_response("seed"), success_response("repop")];
        let response_origins = vec![ScottGaOrigin::Seed, ScottGaOrigin::RePopR];
        let boundary_population = vec![member("seed", ScottGaOrigin::Seed, true)];
        let working_population = vec![member("repop", ScottGaOrigin::RePopR, true)];

        let metrics = build_rust_janus_origin_metrics(
            0,
            "initialize",
            &response_origins,
            &responses,
            &boundary_population,
            &working_population,
        );

        let seed = metrics
            .iter()
            .find(|row| row.origin == "SEED")
            .expect("seed row");
        let repopr = metrics
            .iter()
            .find(|row| row.origin == "REPOPR")
            .expect("repopr row");

        assert_eq!(seed.survivor_count, 1);
        assert_eq!(seed.working_survivor_count, 0);
        assert_eq!(repopr.survivor_count, 0);
        assert_eq!(repopr.working_survivor_count, 1);
    }
}
