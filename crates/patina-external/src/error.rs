use patina_types::EvalResult;
use std::path::PathBuf;
use std::time::Duration;
use thiserror::Error;

use crate::types::ExternalProgram;

/// Errors at the external-code adapter edge.
#[derive(Debug, Error)]
pub enum ExternalError {
    #[error("invalid external evaluation request: {reason}")]
    InvalidRequest { reason: String },
    #[error("{program:?} cannot evaluate this candidate: {reason}")]
    UnsupportedCandidate {
        program: ExternalProgram,
        reason: String,
    },
    #[error("{program:?} adapter is not implemented: {reason}")]
    UnsupportedProgram {
        program: ExternalProgram,
        reason: String,
    },
    #[error("{program:?} process failed (exit_code={exit_code:?}): {stderr}")]
    ProcessFailed {
        program: ExternalProgram,
        exit_code: Option<i32>,
        stderr: String,
    },
    #[error("{program:?} output parse failed: {reason}")]
    ParseFailed {
        program: ExternalProgram,
        reason: String,
    },
    #[error("{program:?} did not converge after {n_steps} steps (energy={energy})")]
    NotConverged {
        program: ExternalProgram,
        energy: f64,
        n_steps: usize,
        partial_result: Option<Box<EvalResult>>,
    },
    #[error("{program:?} timed out after {elapsed:?}")]
    Timeout {
        program: ExternalProgram,
        elapsed: Duration,
    },
    #[error("{program:?} template invalid `{path}`: {reason}")]
    TemplateInvalid {
        program: ExternalProgram,
        path: PathBuf,
        reason: String,
    },
    #[error("external adapter I/O failure: {0}")]
    Io(#[from] std::io::Error),
}
