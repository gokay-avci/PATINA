use patina_external::EvalError;
use patina_types::{Candidate, EvalResult, WorkerOutcome, WorkerRequest, WorkerResponse};
use rayon::prelude::*;
use rayon::ThreadPoolBuilder;

use crate::runtime::{
    create_dir, ensure_workdir, map_eval_error, remove_dir_all_with_warn,
    remove_empty_dir_with_warn, unique_run_name,
};
use crate::{RunConfig, RunnerError};

/// Evaluates a population with isolated worker sandboxes.
pub fn evaluate_population(
    candidates: &[Candidate],
    cfg: &RunConfig,
) -> Result<Vec<Result<EvalResult, EvalError>>, RunnerError> {
    if cfg.n_workers == 0 {
        return Err(RunnerError::InvalidWorkerCount);
    }

    ensure_workdir(&cfg.workdir)?;

    let run_dir = cfg.workdir.join(unique_run_name());
    create_dir(&run_dir)?;

    let pool = ThreadPoolBuilder::new()
        .num_threads(cfg.n_workers)
        .build()
        .map_err(RunnerError::ThreadPoolBuild)?;

    let results = pool.install(|| {
        candidates
            .par_iter()
            .enumerate()
            .map(|(index, candidate)| {
                let sandbox = run_dir.join(format!("worker_{index:04}"));
                create_dir(&sandbox)?;

                let outcome = cfg.backend.evaluate(candidate, &sandbox);
                if !cfg.keep_dirs {
                    remove_dir_all_with_warn(&sandbox);
                }
                Ok(outcome)
            })
            .collect::<Result<Vec<_>, RunnerError>>()
    })?;

    if !cfg.keep_dirs {
        remove_empty_dir_with_warn(&run_dir);
    }

    Ok(results)
}

/// Evaluates typed worker requests with isolated worker sandboxes.
pub fn evaluate_requests(
    requests: &[WorkerRequest],
    cfg: &RunConfig,
) -> Result<Vec<WorkerResponse>, RunnerError> {
    if cfg.n_workers == 0 {
        return Err(RunnerError::InvalidWorkerCount);
    }

    ensure_workdir(&cfg.workdir)?;

    let run_dir = cfg.workdir.join(unique_run_name());
    create_dir(&run_dir)?;

    let pool = ThreadPoolBuilder::new()
        .num_threads(cfg.n_workers)
        .build()
        .map_err(RunnerError::ThreadPoolBuild)?;

    let responses = pool.install(|| {
        requests
            .par_iter()
            .enumerate()
            .map(|(index, request)| {
                let sandbox = run_dir.join(format!("worker_{index:04}"));
                create_dir(&sandbox)?;

                let response = WorkerResponse {
                    request_id: request.request_id.clone(),
                    generation: request.generation,
                    worker_slot: Some(index),
                    outcome: match cfg.backend.evaluate(&request.candidate, &sandbox) {
                        Ok(result) => WorkerOutcome::Success { result },
                        Err(error) => WorkerOutcome::Failure {
                            error: map_eval_error(&error),
                        },
                    },
                };

                if !cfg.keep_dirs {
                    remove_dir_all_with_warn(&sandbox);
                }
                Ok(response)
            })
            .collect::<Result<Vec<_>, RunnerError>>()
    })?;

    if !cfg.keep_dirs {
        remove_empty_dir_with_warn(&run_dir);
    }

    Ok(responses)
}
