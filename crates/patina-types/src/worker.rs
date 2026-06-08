use crate::{Candidate, EvalResult};
use serde::{Deserialize, Serialize};

/// Stable request envelope for a backend worker evaluation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkerRequest {
    /// Unique request identifier within a run or generation.
    pub request_id: String,
    /// Optional generation index associated with the request.
    pub generation: Option<usize>,
    /// Candidate to evaluate.
    pub candidate: Candidate,
}

/// Serializable failure categories for worker protocol responses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkerFailureKind {
    /// External process launch or execution failed.
    ProcessFailed,
    /// Backend output could not be parsed.
    ParseFailed,
    /// Backend completed but did not converge.
    NotConverged,
    /// Backend timed out.
    Timeout,
    /// Filesystem or sandbox preparation failed.
    Io,
    /// Static backend or template configuration was invalid.
    TemplateInvalid,
    /// Internal control-plane failure outside backend semantics.
    Internal,
}

/// Serializable failure payload for worker protocol responses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkerFailure {
    /// Coarse failure class for routing and telemetry.
    pub kind: WorkerFailureKind,
    /// Human-readable detail for logs or UI.
    pub message: String,
}

/// Serializable worker evaluation outcome.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum WorkerOutcome {
    /// Successful backend evaluation.
    Success { result: EvalResult },
    /// Backend or control-plane failure.
    Failure { error: WorkerFailure },
}

/// Stable response envelope for a backend worker evaluation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkerResponse {
    /// Request identifier echoed back to the caller.
    pub request_id: String,
    /// Optional generation index associated with the request.
    pub generation: Option<usize>,
    /// Worker slot that handled this request, when known.
    pub worker_slot: Option<usize>,
    /// Outcome of the request.
    pub outcome: WorkerOutcome,
}
