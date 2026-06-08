use crate::{evaluate_population, evaluate_requests, PersistentWorkerPool, RunConfig};
use patina_external::{BackendEvaluator, EvalError};
use patina_types::{
    Candidate, EvalResult, WorkerFailure, WorkerFailureKind, WorkerOutcome, WorkerRequest,
};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tempfile::tempdir;

#[derive(Debug)]
struct ControlledBackend;

impl BackendEvaluator for ControlledBackend {
    fn evaluate(&self, candidate: &Candidate, workdir: &Path) -> Result<EvalResult, EvalError> {
        if candidate.label == "fail" {
            return Err(EvalError::NotConverged {
                energy: 99.0,
                n_steps: 200,
                partial_result: None,
            });
        }

        fs::write(workdir.join("touch.txt"), candidate.label.as_bytes())
            .map_err(EvalError::IoError)?;
        Ok(EvalResult {
            energy: -(candidate.label.len() as f64),
            forces: vec![[0.0, 0.0, 0.0]; candidate.len()],
            relaxed_candidate: candidate.clone(),
            converged: true,
            wall_time: std::time::Duration::from_secs(1),
        })
    }
}

fn candidate(label: &str) -> Candidate {
    Candidate::cluster(
        label,
        vec!["Ce".into(), "O".into()],
        vec![[0.0, 0.0, 0.0], [0.5, 0.5, 0.5]],
    )
}

fn request(id: &str, generation: usize, label: &str) -> WorkerRequest {
    WorkerRequest {
        request_id: id.into(),
        generation: Some(generation),
        candidate: candidate(label),
    }
}

#[test]
fn evaluate_population_returns_one_result_per_candidate() {
    let temp = tempdir().expect("tempdir");
    let cfg = RunConfig {
        backend: Arc::new(ControlledBackend),
        workdir: temp.path().join("scratch"),
        n_workers: 2,
        keep_dirs: true,
    };

    let results = evaluate_population(&[candidate("a"), candidate("bb"), candidate("ccc")], &cfg)
        .expect("evaluate population");

    assert_eq!(results.len(), 3);
    assert!(results.iter().all(|entry| entry.is_ok()));
}

#[test]
fn evaluate_population_isolates_backend_failures() {
    let temp = tempdir().expect("tempdir");
    let cfg = RunConfig {
        backend: Arc::new(ControlledBackend),
        workdir: temp.path().join("scratch"),
        n_workers: 3,
        keep_dirs: false,
    };

    let results = evaluate_population(
        &[candidate("ok-1"), candidate("fail"), candidate("ok-2")],
        &cfg,
    )
    .expect("evaluate population");

    assert!(results[0].is_ok());
    assert!(matches!(results[1], Err(EvalError::NotConverged { .. })));
    assert!(results[2].is_ok());
}

#[test]
fn evaluate_population_cleans_up_sandboxes_when_requested() {
    let temp = tempdir().expect("tempdir");
    let workdir = temp.path().join("scratch");
    let cfg = RunConfig {
        backend: Arc::new(ControlledBackend),
        workdir: workdir.clone(),
        n_workers: 2,
        keep_dirs: false,
    };

    let _ = evaluate_population(&[candidate("a"), candidate("b")], &cfg).expect("evaluate");

    let entries = fs::read_dir(&workdir)
        .expect("workdir exists")
        .collect::<Result<Vec<_>, _>>()
        .expect("read entries");
    assert!(
        entries.is_empty(),
        "expected cleaned workdir, found {entries:?}"
    );
}

#[test]
fn persistent_pool_reuses_fixed_worker_directories() {
    let temp = tempdir().expect("tempdir");
    let cfg = RunConfig {
        backend: Arc::new(ControlledBackend),
        workdir: temp.path().join("scratch"),
        n_workers: 2,
        keep_dirs: true,
    };
    let pool = PersistentWorkerPool::new(cfg).expect("persistent pool");

    let _ = pool
        .evaluate_population(&[
            candidate("a"),
            candidate("bb"),
            candidate("ccc"),
            candidate("dddd"),
        ])
        .expect("evaluate population");
    let _ = pool
        .evaluate_population(&[candidate("ee"), candidate("fff")])
        .expect("evaluate population again");

    let entries = fs::read_dir(pool.session_dir())
        .expect("session dir")
        .collect::<Result<Vec<_>, _>>()
        .expect("read worker dirs");
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().all(|entry| entry.path().is_dir()));
}

#[test]
fn evaluate_requests_returns_serializable_failure_envelopes() {
    let temp = tempdir().expect("tempdir");
    let cfg = RunConfig {
        backend: Arc::new(ControlledBackend),
        workdir: temp.path().join("scratch"),
        n_workers: 2,
        keep_dirs: false,
    };

    let responses = evaluate_requests(
        &[request("req-ok", 1, "ok"), request("req-fail", 1, "fail")],
        &cfg,
    )
    .expect("evaluate requests");

    assert_eq!(responses.len(), 2);
    assert!(matches!(
        responses[0].outcome,
        WorkerOutcome::Success { .. }
    ));
    assert!(matches!(
        responses[1].outcome,
        WorkerOutcome::Failure {
            error: WorkerFailure {
                kind: WorkerFailureKind::NotConverged,
                ..
            }
        }
    ));
}

#[test]
fn persistent_pool_evaluates_typed_requests() {
    let temp = tempdir().expect("tempdir");
    let cfg = RunConfig {
        backend: Arc::new(ControlledBackend),
        workdir: temp.path().join("scratch"),
        n_workers: 2,
        keep_dirs: true,
    };
    let pool = PersistentWorkerPool::new(cfg).expect("persistent pool");

    let responses = pool
        .evaluate_requests(&[
            request("req-1", 2, "alpha"),
            request("req-2", 2, "fail"),
            request("req-3", 2, "beta"),
        ])
        .expect("evaluate typed requests");

    assert_eq!(responses.len(), 3);
    assert!(responses
        .iter()
        .all(|response| response.worker_slot.is_some()));
    assert!(responses.iter().any(|response| {
        matches!(
            response.outcome,
            WorkerOutcome::Failure {
                error: WorkerFailure {
                    kind: WorkerFailureKind::NotConverged,
                    ..
                }
            }
        )
    }));
}
