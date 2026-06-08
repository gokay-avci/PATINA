use patina_external::EvalError;
use patina_types::{Candidate, EvalResult, WorkerOutcome, WorkerRequest, WorkerResponse};
use rayon::prelude::*;
use rayon::{ThreadPool, ThreadPoolBuilder};
use std::path::Path;

use crate::runtime::{
    create_dir, create_persistent_sandbox, ensure_workdir, map_eval_error,
    remove_session_dir_with_warn, unique_run_name,
};
use crate::{RunConfig, RunnerError};

/// Persistent worker pool that keeps a fixed Rayon thread pool and stable worker sandboxes.
pub struct PersistentWorkerPool {
    cfg: RunConfig,
    pool: ThreadPool,
    session_dir: std::path::PathBuf,
}

impl PersistentWorkerPool {
    /// Creates a persistent pool with one stable workspace per worker thread.
    pub fn new(cfg: RunConfig) -> Result<Self, RunnerError> {
        if cfg.n_workers == 0 {
            return Err(RunnerError::InvalidWorkerCount);
        }

        ensure_workdir(&cfg.workdir)?;

        let session_dir = cfg.workdir.join(format!("pool_{}", unique_run_name()));
        create_dir(&session_dir)?;

        for worker in 0..cfg.n_workers {
            let sandbox = session_dir.join(format!("worker_{worker:04}"));
            create_dir(&sandbox)?;
        }

        let pool = ThreadPoolBuilder::new()
            .num_threads(cfg.n_workers)
            .build()
            .map_err(RunnerError::ThreadPoolBuild)?;

        Ok(Self {
            cfg,
            pool,
            session_dir,
        })
    }

    /// Evaluates a population using stable worker directories reused across batches.
    pub fn evaluate_population(
        &self,
        candidates: &[Candidate],
    ) -> Result<Vec<Result<EvalResult, EvalError>>, RunnerError> {
        let results = self.pool.install(|| {
            candidates
                .par_iter()
                .map(|candidate| {
                    let worker_index = rayon::current_thread_index()
                        .ok_or(RunnerError::MissingWorkerThreadIndex)?;
                    let sandbox = self.session_dir.join(format!("worker_{worker_index:04}"));
                    create_persistent_sandbox(&sandbox)?;

                    let outcome = self.cfg.backend.evaluate(candidate, &sandbox);
                    if !self.cfg.keep_dirs {
                        create_persistent_sandbox(&sandbox)?;
                    }
                    Ok(outcome)
                })
                .collect::<Result<Vec<_>, RunnerError>>()
        })?;

        Ok(results)
    }

    /// Evaluates typed worker requests using stable worker directories reused across batches.
    pub fn evaluate_requests(
        &self,
        requests: &[WorkerRequest],
    ) -> Result<Vec<WorkerResponse>, RunnerError> {
        self.pool.install(|| {
            requests
                .par_iter()
                .map(|request| {
                    let worker_index = rayon::current_thread_index()
                        .ok_or(RunnerError::MissingWorkerThreadIndex)?;
                    let sandbox = self.session_dir.join(format!("worker_{worker_index:04}"));
                    create_persistent_sandbox(&sandbox)?;

                    let response = WorkerResponse {
                        request_id: request.request_id.clone(),
                        generation: request.generation,
                        worker_slot: Some(worker_index),
                        outcome: match self.cfg.backend.evaluate(&request.candidate, &sandbox) {
                            Ok(result) => WorkerOutcome::Success { result },
                            Err(error) => WorkerOutcome::Failure {
                                error: map_eval_error(&error),
                            },
                        },
                    };

                    if !self.cfg.keep_dirs {
                        create_persistent_sandbox(&sandbox)?;
                    }
                    Ok(response)
                })
                .collect::<Result<Vec<_>, RunnerError>>()
        })
    }

    /// Returns the stable session directory used by this pool.
    pub fn session_dir(&self) -> &Path {
        &self.session_dir
    }
}

impl Drop for PersistentWorkerPool {
    fn drop(&mut self) {
        if !self.cfg.keep_dirs {
            remove_session_dir_with_warn(&self.session_dir);
        }
    }
}
