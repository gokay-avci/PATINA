use anyhow::Result;

use patina_types::{WorkerRequest, WorkerResponse};

use crate::application::ga_execution::RustJanusGaExecution;

/// Application-layer port for evaluating one GA batch.
///
/// Implementations may use persistent daemon workers, the monolithic Scott runtime,
/// or a future native Scott adapter, but the workflow service should not need
/// to know which concrete mechanism is behind the evaluation.
pub trait GaEvaluationPort {
    fn evaluate_generation(&self, requests: &[WorkerRequest]) -> Result<Vec<WorkerResponse>>;
}

/// Application-layer port for persisting GA workflow artifacts.
///
/// Workflow execution should produce domain/application results only. Concrete
/// filesystem layout and report emission belong behind this sink boundary.
pub trait GaArtifactSink {
    fn persist_generation(
        &self,
        generation_index: usize,
        execution: &RustJanusGaExecution,
    ) -> Result<()>;
}

/// Application-layer port for final GA run artifact persistence.
pub trait GaRunArtifactSink {
    type Summary;

    fn persist_run(&self, execution: &RustJanusGaExecution) -> Result<Self::Summary>;
}
